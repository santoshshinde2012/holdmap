/** Quote original argv for a user to paste into their shell; display text is not argv. */
export function commandForShell(args: readonly string[], cwd?: string | null, platform = ""): string {
  const windows = platform === "windows" || ((platform === "" || platform === "browser") && typeof navigator !== "undefined" && /Win/.test(navigator.platform));
  if (!args.length) return "";
  if (windows) {
    // Supports Windows PowerShell 5.1 and PowerShell 7. Their native argument binders
    // differ: 5.1/Legacy drops empty strings and embedded quotes. Start the executable
    // directly with .NET and Windows argv quoting instead of invoking that binder.
    const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
    // Backslashes before a quote are doubled, plus one to escape the quote; trailing
    // backslashes are doubled before the closing delimiter (the Windows CRT rules).
    const nativeQuote = (value: string) => `"${value.replace(/(\\*)"/g, (_, slashes: string) => `${slashes}${slashes}\\"`).replace(/(\\+)$/g, "$1$1")}"`;
    // WorkingDirectory doesn't govern executable lookup. Resolve explicit paths
    // against it; leave bare names intact so the child inherits normal PATH lookup.
    const executable = /[\\/]/.test(args[0]) ? `[System.IO.Path]::GetFullPath([System.IO.Path]::Combine($holdmapStart.WorkingDirectory, ${quote(args[0])}))` : quote(args[0]);
    return `& { $ErrorActionPreference = 'Stop'; $holdmapStart = New-Object System.Diagnostics.ProcessStartInfo; $holdmapStart.WorkingDirectory = ${cwd ? quote(cwd) : "(Get-Location).ProviderPath"}; $holdmapStart.FileName = ${executable}; $holdmapStart.Arguments = ${quote(args.slice(1).map(nativeQuote).join(" "))}; $holdmapStart.UseShellExecute = $false; $holdmapProcess = [System.Diagnostics.Process]::Start($holdmapStart); try { $holdmapProcess.WaitForExit(); $global:LASTEXITCODE = $holdmapProcess.ExitCode } finally { $holdmapProcess.Dispose() } }`;
  }
  const literal = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;
  const quote = (value: string) => /^[A-Za-z0-9_./:@%+=,-]+$/.test(value) ? value : literal(value);
  // Even simple executable names can be shell syntax (if, time, NAME=value).
  const command = [literal(args[0]), ...args.slice(1).map(quote)].join(" ");
  return cwd ? `cd -- ${quote(cwd)} && ${command}` : command;
}
