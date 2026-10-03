// Form validation shared by the desktop forms. Messages are user-facing and say how to fix it.
// `validateHost` mirrors `portwise_core::remote::validate_host` (the backend re-checks).

export function validatePort(input: string | number | null | undefined): string | null {
  const s = String(input ?? "").trim();
  if (!s) return "Enter a port number";
  if (!/^\d+$/.test(s)) return "Ports are whole numbers, like 3000";
  const n = Number(s);
  if (n < 1 || n > 65535) return "Ports go from 1 to 65535";
  return null;
}

export function validateRange(n: number | null, min: number, max: number, unit = ""): string | null {
  if (n === null || Number.isNaN(n)) return `Enter a number from ${min} to ${max}${unit}`;
  if (!Number.isInteger(n)) return "Use a whole number";
  if (n < min || n > max) return `Choose ${min}–${max}${unit}`;
  return null;
}

export function validateLabel(s: string, max = 40): string | null {
  return [...s.trim()].length > max ? `Keep it to ${max} characters or fewer` : null;
}

const NAME = /^[A-Za-z0-9._-]+$/;
const USER = /^[A-Za-z0-9._-]+$/;

export function validateHost(input: string): string | null {
  const h = input.trim();
  if (!h) return "Enter a host, like devbox or user@10.0.0.5";
  if (h.length > 255) return "That host name is too long";
  if (h.startsWith("-")) return "A host can't start with “-”";
  const at = h.lastIndexOf("@");
  const user = at >= 0 ? h.slice(0, at) : null;
  const rest = at >= 0 ? h.slice(at + 1) : h;
  if (user !== null && (!user || !USER.test(user))) return "The user name before “@” has unsupported characters";
  let host: string, port: string | null = null;
  if (rest.startsWith("[")) {
    const end = rest.indexOf("]");
    if (end < 0) return "Close the IPv6 address with “]”";
    host = rest.slice(1, end);
    if (!/^[0-9a-fA-F:.]+$/.test(host) || !host.includes(":")) return "That isn't a valid IPv6 address";
    const tail = rest.slice(end + 1);
    if (tail) { if (!tail.startsWith(":")) return "That isn't a valid IPv6 address"; port = tail.slice(1); }
  } else {
    const c = rest.lastIndexOf(":");
    host = c >= 0 ? rest.slice(0, c) : rest;
    port = c >= 0 ? rest.slice(c + 1) : null;
  }
  if (port !== null && (!/^\d+$/.test(port) || +port < 1 || +port > 65535)) return "The SSH port after “:” must be a number from 1 to 65535";
  if (!host) return "Enter a host name after “@”";
  if (!rest.startsWith("[") && (host.startsWith("-") || !NAME.test(host))) return "Host names may only contain letters, digits, “.”, “-” and “_”";
  return null;
}
