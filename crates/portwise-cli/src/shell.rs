//! Shell integration: `portwise init zsh|bash|fish|powershell` prints a hook that, when a
//! command fails, asks the hidden `portwise hint` whether it failed because a port it wanted is
//! taken, and if so says who holds it and how to free it.

use crate::exit;
use crate::style::{bold, dim, paint, S};
use anyhow::Result;
use portwise_core::hint::ports_in_command;
use portwise_core::*;

/// Shells `portwise init` supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum InitShell {
    Zsh,
    Bash,
    Fish,
    #[value(alias = "pwsh")]
    Powershell,
}

const ZSH: &str = r#"# portwise shell integration for zsh. Add to ~/.zshrc:  eval "$(portwise init zsh)"
__portwise_preexec() { __portwise_cmd="$1"; }
__portwise_precmd() {
  local st=$?
  if (( st != 0 && st < 128 )) && [[ -n "$__portwise_cmd" ]]; then
    command portwise hint --exit-code "$st" -- "$__portwise_cmd"
  fi
  __portwise_cmd=
}
autoload -Uz add-zsh-hook
add-zsh-hook preexec __portwise_preexec
add-zsh-hook precmd __portwise_precmd
"#;

const BASH: &str = r#"# portwise shell integration for bash. Add to ~/.bashrc:  eval "$(portwise init bash)"
__portwise_last_hist=
__portwise_prompt() {
  local st=$? line num
  line=$(HISTTIMEFORMAT= builtin history 1)
  num=${line%%[^ 0-9]*}
  if [ "$st" -ne 0 ] && [ "$st" -lt 128 ] && [ -n "$num" ] && [ "$num" != "$__portwise_last_hist" ]; then
    command portwise hint --exit-code "$st" -- "${line#"${line%%[^ 0-9]*}"}"
  fi
  __portwise_last_hist=$num
  return $st
}
case ";${PROMPT_COMMAND-};" in
  *";__portwise_prompt;"*) ;;
  *) PROMPT_COMMAND="__portwise_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
esac
"#;

const FISH: &str = r#"# portwise shell integration for fish. Add to ~/.config/fish/config.fish:  portwise init fish | source
function __portwise_postexec --on-event fish_postexec
    set -l st $status
    if test $st -ne 0 -a $st -lt 128
        command portwise hint --exit-code $st -- "$argv"
    end
end
"#;

const POWERSHELL: &str = r#"# portwise shell integration for PowerShell. Add to $PROFILE:  Invoke-Expression (& portwise init powershell | Out-String)
if (-not $global:__portwiseOriginalPrompt) { $global:__portwiseOriginalPrompt = $function:prompt }
$global:__portwiseLastId = 0
function global:prompt {
    $ok = $?
    $code = $global:LASTEXITCODE
    $h = Get-History -Count 1
    if ($h -and $h.Id -ne $global:__portwiseLastId) {
        $global:__portwiseLastId = $h.Id
        if (-not $ok -and $code -and $code -ne 0 -and $code -lt 128) {
            & portwise hint --exit-code $code -- $h.CommandLine
            $global:LASTEXITCODE = $code
        }
    }
    & $global:__portwiseOriginalPrompt
}
"#;

/// The integration script for `shell`.
pub fn script(shell: InitShell) -> &'static str {
    match shell {
        InitShell::Zsh => ZSH,
        InitShell::Bash => BASH,
        InitShell::Fish => FISH,
        InitShell::Powershell => POWERSHELL,
    }
}

/// `portwise hint -- <command line>`: explain busy ports the failed command wanted. Prints to
/// stderr, and nothing at all when no port of that command is busy. Always exits 0 so it never
/// changes the shell's `$?`.
pub fn hint(command: &[String], docker: bool) -> Result<u8> {
    let line = command.join(" ");
    let busy: Vec<u16> = ports_in_command(&line)
        .into_iter()
        .take(4)
        .filter(|p| port_busy(*p, Protocol::Tcp))
        .collect();
    if busy.is_empty() {
        return Ok(exit::OK);
    }
    let Ok(e) = Engine::new(&ScanOptions {
        all_states: false,
        docker,
    }) else {
        return Ok(exit::OK);
    };
    for p in busy {
        let ex = e.explain(p, &StopOptions::default());
        if ex.status == PortStatus::Free {
            continue;
        }
        eprintln!(
            "{} {}",
            paint("portwise:", S::BoldYellow),
            bold(&ex.headline)
        );
        eprintln!(
            "  {} {}  {}  {}",
            dim("→"),
            paint(format!("portwise stop {p}"), S::Cyan),
            dim("frees it ·"),
            paint(format!("portwise free-port --near {p}"), S::Cyan)
        );
    }
    Ok(exit::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_call_the_hint_command() {
        for sh in [
            InitShell::Zsh,
            InitShell::Bash,
            InitShell::Fish,
            InitShell::Powershell,
        ] {
            let s = script(sh);
            assert!(s.contains("portwise hint --exit-code"), "{sh:?}");
            assert!(s.starts_with("# portwise shell integration"), "{sh:?}");
        }
    }

    /// Syntax-check the scripts with the real shells when they're installed.
    #[cfg(unix)]
    #[test]
    fn scripts_parse() {
        for (sh, args, src) in [
            ("bash", vec!["-n"], BASH),
            ("zsh", vec!["-n"], ZSH),
            ("fish", vec!["--no-execute"], FISH),
        ] {
            let Ok(mut child) = std::process::Command::new(sh)
                .args(&args)
                .stdin(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            else {
                continue; // shell not installed
            };
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(src.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "{sh}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
