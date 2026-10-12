import { execFileSync, spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { describe, expect, it } from "vitest";
import { commandForShell } from "./shell";

describe("copied restart commands", () => {
  it.skipIf(process.platform === "win32")("preserves every argument literally when pasted into a POSIX shell", () => {
    const args = ["", "two words", "'single'", '"double"', "$(printf injected)", "`printf injected`", "semi;colon", "\nnewline", "$HOME", "*", "\\path"];
    const command = commandForShell([process.execPath, "-e", "process.stdout.write(JSON.stringify(process.argv.slice(1)))", ...args], process.cwd(), "linux");
    expect(JSON.parse(execFileSync("/bin/sh", ["-c", command], { encoding: "utf8" }))).toEqual(args);
  });

  it.skipIf(process.platform === "win32")("invokes assignment-like and keyword executable names literally", () => {
    const folder = mkdtempSync(join(tmpdir(), "holdmap-shell-"));
    try {
      for (const executable of ["VALUE=injected", "if", "time"]) {
        const path = join(folder, executable);
        writeFileSync(path, '#!/bin/sh\nprintf "%s" "$1"\n');
        chmodSync(path, 0o700);
        const command = commandForShell([executable, "literal executable"], null, "linux");
        expect(execFileSync("/bin/sh", ["-c", command], { encoding: "utf8", env: { ...process.env, PATH: `${folder}:${process.env.PATH ?? ""}` } })).toBe("literal executable");
      }
    } finally { rmSync(folder, { recursive: true, force: true }); }
  });

  it("quotes working directories and executable paths including substitution syntax", () => {
    expect(commandForShell(["/tmp/two words/bin", "--port", "3000"], "/tmp/$(touch marker)`id`'path", "macos"))
      .toBe("cd -- '/tmp/$(touch marker)`id`'\\''path' && '/tmp/two words/bin' --port 3000");
  });

  it("uses literal PowerShell strings and explicitly quoted native arguments", () => {
    const command = commandForShell(["C:\\Program Files\\node.exe", "$(Write-Output injected)", "a'b", "", 'embedded "quote"', "C:\\trailing space\\"], "C:\\work's [dir]", "windows");
    expect(command).toContain("$holdmapStart.FileName = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($holdmapStart.WorkingDirectory, 'C:\\Program Files\\node.exe'))");
    expect(command).toContain('$holdmapStart.Arguments = \'"$(Write-Output injected)" "a\'\'b" "" "embedded \\"quote\\"" "C:\\trailing space\\\\"\'');
    expect(command).toContain("$holdmapStart.WorkingDirectory = 'C:\\work''s [dir]'");
    expect(command).toContain("$holdmapStart.UseShellExecute = $false");
  });

  for (const [shell, edition, major] of [["powershell.exe", "Desktop", 5], ["pwsh.exe", "Core", 7]] as const) {
    it.skipIf(process.platform !== "win32")(`preserves native argv, cwd, output and exit status in ${shell}`, () => {
      const folder = mkdtempSync(join(tmpdir(), "holdmap's [cwd] $literal-"));
      const report = join(folder, "result.json");
      const fixture = join(folder, "argv fixture.cjs");
      const bin = join(folder, "bin");
      const work = join(folder, "work");
      const args = ["", "two words", "   ", 'embedded "quote"', 'backslash\\"quote', "a'b", "$HOME", "$(Write-Output injected)", "`Write-Output injected`", "semi;colon", "C:\\trailing space\\", "plain\\", "line\r\nbreak", "नमस्ते"];
      try {
        mkdirSync(bin); mkdirSync(work);
        copyFileSync(process.execPath, join(bin, "tool.exe"));
        writeFileSync(fixture, "require('node:fs').writeFileSync(process.env.HOLDMAP_SHELL_RESULT, JSON.stringify({ argv: process.argv.slice(2), cwd: process.cwd() })); process.stdout.write('stdout inherited\\n'); process.stderr.write('stderr inherited\\n'); process.exitCode = 17;");
        const env: NodeJS.ProcessEnv = { ...process.env, HOLDMAP_SHELL_RESULT: report };
        const pathKey = Object.keys(env).find((key) => key.toLowerCase() === "path") ?? "PATH";
        env[pathKey] = `${bin}${delimiter}${env[pathKey] ?? ""}`;
        for (const [executable, cwd] of [[process.execPath, folder], [".\\tool.exe", bin], ["..\\bin\\tool.exe", work], ["tool.exe", work]]) {
          const command = commandForShell([executable, fixture, ...args], cwd, "windows");
          // EncodedCommand transports the test script without introducing a second
          // native quoting layer. The helper itself must invoke a real native process.
          const script = `[ordered]@{ edition = $PSVersionTable.PSEdition; major = $PSVersionTable.PSVersion.Major } | ConvertTo-Json -Compress; ${command}; if ($LASTEXITCODE -ne 17) { throw 'Native exit status was lost' }`;
          const result = spawnSync(shell, ["-NoProfile", "-NonInteractive", "-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")], { encoding: "utf8", timeout: 20_000, env });
          expect(result.error).toBeUndefined();
          expect(result.status, result.stderr).toBe(0);
          const version = JSON.parse(result.stdout.split(/\r?\n/)[0]);
          expect(version.edition).toBe(edition);
          expect(version.major).toBeGreaterThanOrEqual(major);
          const observed = JSON.parse(readFileSync(report, "utf8"));
          expect(observed.argv).toEqual(args);
          expect(realpathSync(observed.cwd)).toBe(realpathSync(cwd));
          expect(result.stdout).toContain("stdout inherited");
          expect(result.stderr).toContain("stderr inherited");
        }
      } finally { rmSync(folder, { recursive: true, force: true }); }
    }, 60_000);
  }

  it("never manufactures a command for an empty argument vector", () => {
    expect(commandForShell([], "/tmp", "linux")).toBe("");
  });
});
