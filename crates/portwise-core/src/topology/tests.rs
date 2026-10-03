//! Fixture-based tests for graph construction, clustering, ordering and export.

use super::*;
use crate::docker::{Endpoint, PublishedPort, Runtime};
use crate::model::*;
use crate::process::tests::{proc, table};
use crate::scan::{build_entries, Scan};
use std::fs;
use std::path::Path;

pub(crate) fn listen(port: u16, pids: &[u32]) -> RawSocket {
    RawSocket {
        protocol: Protocol::Tcp,
        family: Family::V4,
        local_addr: "127.0.0.1".parse().unwrap(),
        local_port: port,
        remote_addr: None,
        remote_port: None,
        state: SocketState::Listen,
        uid: Some(1000),
        inode: None,
        pids: pids.to_vec(),
    }
}

/// Both halves of a loopback connection `client_pid:cport → server_pid:sport`.
pub(crate) fn conn(client: u32, cport: u16, server: u32, sport: u16) -> [RawSocket; 2] {
    let lo: std::net::IpAddr = "127.0.0.1".parse().unwrap();
    let mk = |lport, rport, pid| RawSocket {
        protocol: Protocol::Tcp,
        family: Family::V4,
        local_addr: lo,
        local_port: lport,
        remote_addr: Some(lo),
        remote_port: Some(rport),
        state: SocketState::Established,
        uid: Some(1000),
        inode: None,
        pids: vec![pid],
    };
    [mk(cport, sport, client), mk(sport, cport, server)]
}

fn remote(pid: u32, lport: u16, addr: &str, rport: u16) -> RawSocket {
    RawSocket {
        protocol: Protocol::Tcp,
        family: Family::V4,
        local_addr: "10.0.0.5".parse().unwrap(),
        local_port: lport,
        remote_addr: Some(addr.parse().unwrap()),
        remote_port: Some(rport),
        state: SocketState::Established,
        uid: Some(1000),
        inode: None,
        pids: vec![pid],
    }
}

fn at(mut p: ProcessInfo, cwd: &Path) -> ProcessInfo {
    p.cwd = Some(cwd.to_path_buf());
    p
}

pub(crate) fn scan_of(
    procs: Vec<ProcessInfo>,
    raw: Vec<RawSocket>,
    published: Vec<PublishedPort>,
) -> Scan {
    let t = table(procs, 99);
    let (entries, hidden) = build_entries(&raw, &t, &published, false);
    Scan {
        snapshot: Snapshot {
            entries,
            hidden_sockets: hidden,
            platform: "test".into(),
            taken_at_ms: 0,
            scan_ms: 0,
            docker_available: !published.is_empty(),
            warnings: vec![],
        },
        table: t,
        published,
        raw,
    }
}

/// acme-shop: pnpm monorepo with web (vite, :3000) → api (python, :8080) → postgres :5432 +
/// redis :6379, a worker client → redis, outbound HTTPS from api and an inbound LAN client.
pub(crate) fn acme(dir: &Path) -> Scan {
    let root = dir.join("acme-shop");
    for (sub, file, body) in [
        (
            "apps/web",
            "package.json",
            r#"{"name":"web","scripts":{"dev":"vite"},"devDependencies":{"vite":"6"}}"#,
        ),
        (
            "services/api",
            "pyproject.toml",
            "[project]\nname = \"api\"\ndependencies = [\"fastapi\"]\n",
        ),
        ("services/worker", "package.json", r#"{"name":"worker"}"#),
    ] {
        fs::create_dir_all(root.join(sub)).unwrap();
        fs::write(root.join(sub).join(file), body).unwrap();
    }
    fs::write(
        root.join("pnpm-workspace.yaml"),
        "packages: ['apps/*','services/*']\n",
    )
    .unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    let web = root.join("apps/web");
    let api = root.join("services/api");
    let worker = root.join("services/worker");
    let procs = vec![
        proc(1, 0, "init", &[]),
        proc(10, 1, "zsh", &["-zsh"]),
        at(proc(20, 10, "npm", &["npm", "run", "dev"]), &web),
        at(
            proc(21, 20, "node", &["node", "node_modules/.bin/vite"]),
            &web,
        ),
        at(
            proc(
                30,
                10,
                "python3",
                &["python3", "-m", "uvicorn", "main:app", "--port", "8080"],
            ),
            &api,
        ),
        proc(40, 1, "postgres", &["postgres", "-D", "/var/lib/pg"]),
        proc(41, 40, "postgres", &["postgres: checkpointer"]),
        proc(45, 1, "redis-server", &["redis-server *:6379"]),
        at(proc(50, 10, "node", &["node", "worker.js"]), &worker),
        proc(98, 1, "bash", &["bash"]),
        proc(99, 98, "portwise", &["portwise", "graph"]),
    ];
    let mut raw = vec![
        listen(3000, &[21]),
        listen(8080, &[30]),
        listen(5432, &[40]),
        listen(6379, &[45]),
    ];
    raw.extend(conn(21, 50001, 30, 8080));
    raw.extend(conn(30, 50002, 40, 5432));
    raw.extend(conn(30, 50003, 41, 5432)); // second backend handled by a postgres child
    raw.extend(conn(30, 50004, 45, 6379));
    raw.extend(conn(50, 50005, 45, 6379));
    raw.push(remote(30, 50100, "140.82.112.3", 443));
    raw.push(remote(21, 3000, "192.168.1.50", 61000)); // LAN browser → web
    scan_of(procs, raw, vec![])
}

fn ids(g: &Graph) -> Vec<&str> {
    g.nodes.iter().map(|n| n.id.as_str()).collect()
}

fn edge<'a>(g: &'a Graph, from: &str, to: &str) -> Option<&'a Edge> {
    g.edges.iter().find(|e| e.from == from && e.to == to)
}

#[test]
fn builds_service_graph_with_dependencies() {
    let tmp = tempfile::tempdir().unwrap();
    let g = TopologyBuilder::new(&acme(tmp.path())).build();
    let mut got = ids(&g);
    got.sort();
    assert_eq!(
        got,
        ["external", "svc:20", "svc:30", "svc:40", "svc:45", "svc:50"]
    );
    let web = g.node("svc:20").expect("web is keyed by its npm launcher");
    assert_eq!(web.label, "web");
    assert_eq!(web.kind, NodeKind::Service);
    assert_eq!(web.ports[0].port, 3000);
    assert!(web.pids.contains(&21));
    let worker = g.node("svc:50").unwrap();
    assert_eq!(worker.kind, NodeKind::Client);

    let e = edge(&g, "svc:20", "svc:30").expect("web → api");
    assert_eq!((e.kind, e.port, e.connections), (EdgeKind::Local, 8080, 1));
    let e = edge(&g, "svc:30", "svc:40").expect("api → postgres (both backends)");
    assert_eq!(e.connections, 2);
    assert!(edge(&g, "svc:30", "svc:45").is_some());
    assert!(edge(&g, "svc:50", "svc:45").is_some());
    // Server halves never create reverse edges.
    assert!(edge(&g, "svc:30", "svc:20").is_none());
    assert!(edge(&g, "svc:40", "svc:30").is_none());

    let out = edge(&g, "svc:30", EXTERNAL_ID).expect("outbound collapsed into external");
    assert_eq!((out.kind, out.port), (EdgeKind::Outbound, 443));
    assert_eq!(out.remotes, ["140.82.112.3:443"]);
    let inb = edge(&g, EXTERNAL_ID, "svc:20").expect("inbound LAN client");
    assert_eq!((inb.kind, inb.port), (EdgeKind::Inbound, 3000));

    assert_eq!(
        g.dependents("svc:45")
            .iter()
            .map(|n| n.id.as_str())
            .collect::<Vec<_>>(),
        ["svc:30", "svc:50"]
    );
    assert_eq!(g.dependencies("svc:30").len(), 2);
    assert_eq!(g.stats.connections, 7);
}

#[test]
fn clusters_by_workspace_root() {
    let tmp = tempfile::tempdir().unwrap();
    let g = TopologyBuilder::new(&acme(tmp.path())).build();
    assert_eq!(g.clusters.len(), 1, "{:?}", g.clusters);
    let c = &g.clusters[0];
    assert_eq!(
        (c.name.as_str(), c.kind, c.detail.as_deref()),
        ("acme-shop", ClusterKind::Workspace, Some("pnpm"))
    );
    let mut members = c.nodes.clone();
    members.sort();
    assert_eq!(members, ["svc:20", "svc:30", "svc:50"]);
    assert_eq!(
        g.node("svc:30").unwrap().cluster.as_deref(),
        Some(c.id.as_str())
    );
    assert!(g.node("svc:40").unwrap().cluster.is_none());
    assert_eq!(
        g.find_cluster("cluster:acme-shop").map(|c| c.nodes.len()),
        Some(3)
    );
    assert_eq!(
        g.find_cluster("ACME-SHOP").map(|c| c.kind),
        Some(ClusterKind::Workspace)
    );
}

#[test]
fn git_repo_clusters_need_two_members() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("solo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::write(repo.join("package.json"), r#"{"name":"solo"}"#).unwrap();
    let procs = vec![
        proc(1, 0, "init", &[]),
        at(proc(5, 1, "node", &["node", "a.js"]), &repo),
        proc(99, 1, "portwise", &[]),
    ];
    let g = TopologyBuilder::new(&scan_of(procs.clone(), vec![listen(4000, &[5])], vec![])).build();
    assert!(g.clusters.is_empty());
    let mut procs = procs;
    procs.push(at(proc(6, 1, "node", &["node", "b.js"]), &repo));
    let g = TopologyBuilder::new(&scan_of(
        procs,
        vec![listen(4000, &[5]), listen(4001, &[6])],
        vec![],
    ))
    .build();
    assert_eq!(g.clusters.len(), 1);
    assert_eq!(
        (g.clusters[0].kind, g.clusters[0].name.as_str()),
        (ClusterKind::Git, "solo")
    );
}

#[test]
fn supervisor_children_are_separate_services_in_one_cluster() {
    let procs = vec![
        proc(1, 0, "init", &[]),
        proc(10, 1, "zsh", &["-zsh"]),
        proc(
            60,
            10,
            "node",
            &[
                "node",
                "/app/node_modules/.bin/concurrently",
                "npm:web",
                "npm:api",
            ],
        ),
        proc(61, 60, "npm", &["npm", "run", "web"]),
        proc(62, 61, "node", &["node", "web.js"]),
        proc(63, 60, "npm", &["npm", "run", "api"]),
        proc(64, 63, "node", &["node", "api.js"]),
        proc(99, 1, "portwise", &[]),
    ];
    let mut raw = vec![listen(4000, &[62]), listen(4001, &[64])];
    raw.extend(conn(62, 51000, 64, 4001));
    let g = TopologyBuilder::new(&scan_of(procs, raw, vec![])).build();
    assert!(
        g.node("svc:61").is_some() && g.node("svc:63").is_some(),
        "{:?}",
        ids(&g)
    );
    assert_eq!(g.clusters.len(), 1);
    let c = &g.clusters[0];
    assert_eq!(
        (c.id.as_str(), c.kind, c.detail.as_deref()),
        ("sup:60", ClusterKind::Supervisor, Some("concurrently"))
    );
    assert_eq!(c.nodes.len(), 2);
    assert!(edge(&g, "svc:61", "svc:63").is_some());
}

fn published(port: u16, id: &str, project: &str, service: &str) -> PublishedPort {
    PublishedPort {
        host_port: port,
        protocol: "tcp".into(),
        host_ip: None,
        container: ContainerInfo {
            id: id.into(),
            name: format!("{project}-{service}-1"),
            image: format!("{service}:latest"),
            runtime: "Docker".into(),
            compose_project: Some(project.into()),
            compose_service: Some(service.into()),
            private_port: port,
        },
        runtime: Runtime {
            endpoint: Endpoint::Unix("/var/run/docker.sock".into()),
            label: "Docker".into(),
        },
    }
}

#[test]
fn compose_and_kubernetes_clusters() {
    let procs = vec![
        proc(1, 0, "init", &[]),
        proc(10, 1, "zsh", &["-zsh"]),
        proc(
            70,
            10,
            "kubectl",
            &[
                "kubectl",
                "-n",
                "payments",
                "port-forward",
                "svc/ledger",
                "9000:80",
            ],
        ),
        proc(71, 10, "node", &["node", "app.js"]),
        proc(99, 1, "portwise", &[]),
    ];
    let mut raw = vec![listen(9000, &[70])];
    raw.extend(conn(71, 52000, 70, 9000));
    let mut client = conn(71, 52001, 0, 5433)[0].clone();
    client.pids = vec![71];
    raw.push(client);
    let pubs = vec![
        published(5433, "aaaaaaaaaaaa1111", "shop", "db"),
        published(6380, "bbbbbbbbbbbb2222", "shop", "cache"),
    ];
    let g = TopologyBuilder::new(&scan_of(procs, raw, pubs)).build();
    let compose = g.find_cluster("shop").expect("compose cluster");
    assert_eq!(compose.kind, ClusterKind::Compose);
    assert_eq!(compose.nodes.len(), 2);
    let db = g.node("ctr:aaaaaaaaaaaa").unwrap();
    assert_eq!((db.kind, db.label.as_str()), (NodeKind::Container, "db"));
    assert!(
        edge(&g, "svc:71", "ctr:aaaaaaaaaaaa").is_some(),
        "{:?}",
        g.edges
    );
    let k8s = g.find_cluster("k8s/payments").expect("k8s cluster");
    assert_eq!(k8s.kind, ClusterKind::Kubernetes);
    assert_eq!(k8s.nodes, ["svc:70"]);
    let fwd = g.node("svc:70").unwrap();
    assert_eq!(
        fwd.tunnel.as_ref().unwrap().target,
        "svc/ledger:80 in namespace payments"
    );
}

#[test]
fn stop_order_puts_dependents_first() {
    let tmp = tempfile::tempdir().unwrap();
    let g = TopologyBuilder::new(&acme(tmp.path())).build();
    let all: Vec<String> = ["svc:40", "svc:45", "svc:30", "svc:20", "svc:50"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let o = stop_order(&g, &all);
    let pos = |id: &str| o.order.iter().position(|x| x == id).unwrap();
    assert!(pos("svc:20") < pos("svc:30"), "{:?}", o.order);
    assert!(pos("svc:30") < pos("svc:40"));
    assert!(pos("svc:30") < pos("svc:45"));
    assert!(pos("svc:50") < pos("svc:45"));
    assert!(o.cyclic.is_empty());
    assert_eq!(o.order.len(), 5);
}

#[test]
fn stop_order_breaks_cycles_deterministically() {
    let mk = |from: &str, to: &str| Edge {
        id: format!("{from}->{to}"),
        from: from.into(),
        to: to.into(),
        kind: EdgeKind::Local,
        port: 1,
        connections: 1,
        remotes: vec![],
    };
    let g = Graph {
        edges: vec![mk("a", "b"), mk("b", "a"), mk("c", "a")],
        ..Default::default()
    };
    let ids: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    let o = stop_order(&g, &ids);
    assert_eq!(o.order, ["c", "a", "b"]);
    assert_eq!(o.cyclic, ["a"]);
    assert_eq!(stop_order(&g, &ids), o, "deterministic");
}

#[test]
fn exporters_render_all_formats() {
    let tmp = tempfile::tempdir().unwrap();
    let g = TopologyBuilder::new(&acme(tmp.path())).build();
    let dot = exporter("dot").unwrap().export(&g);
    assert!(dot.starts_with("digraph portwise {"));
    assert!(dot.contains("subgraph cluster_0"));
    assert!(
        dot.contains("\"svc:20\" -> \"svc:30\" [label=\":8080\"]"),
        "{dot}"
    );
    assert!(dot.contains("style=dashed"));
    let mmd = exporter("mermaid").unwrap().export(&g);
    assert!(mmd.starts_with("flowchart LR"));
    assert!(mmd.contains("svc_20 -->|\":8080\"| svc_30"), "{mmd}");
    assert!(mmd.contains("external((\"External hosts\"))"));
    let tree = exporter("tree").unwrap().export(&g);
    assert!(tree.contains("acme-shop (workspace, pnpm)"), "{tree}");
    assert!(tree.contains("→ api :8080"), "{tree}");
    assert!(tree.contains("→ External hosts 140.82.112.3:443"), "{tree}");
    let ascii = TreeExporter { ascii: true }.export(&g);
    assert!(ascii.is_ascii(), "{ascii}");
    let json: Graph = serde_json::from_str(&exporter("json").unwrap().export(&g)).unwrap();
    assert_eq!(json, g);
    assert!(exporter("png").is_none());
}

#[test]
fn retain_dev_drops_unrelated_nodes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = TopologyBuilder::new(&acme(tmp.path())).build();
    let before = g.nodes.len();
    g.retain(|n| n.id != "svc:45");
    assert_eq!(g.nodes.len(), before - 1);
    assert!(g
        .edges
        .iter()
        .all(|e| e.from != "svc:45" && e.to != "svc:45"));
    g.retain_dev();
    assert!(g.node("svc:20").is_some());
    assert_eq!(g.stats.nodes, g.nodes.len());
}
