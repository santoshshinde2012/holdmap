---
title: Shell hook
description: Make "address already in use" errors explain themselves with a one-line shell integration.
order: 4
---

Add one line to your shell's startup file. When a command that starts a server fails because its
port is taken, portwise prints who holds the port and how to free it.

| Shell | Add to | Line |
|---|---|---|
| zsh | `~/.zshrc` | `eval "$(portwise init zsh)"` |
| bash | `~/.bashrc` | `eval "$(portwise init bash)"` |
| fish | `~/.config/fish/config.fish` | `portwise init fish \| source` |
| PowerShell | `$PROFILE` | `Invoke-Expression (& portwise init powershell \| Out-String)` |

Open a new terminal, then:

```text
$ PORT=3000 npm run dev
Error: listen EADDRINUSE: address already in use :::3000
portwise: Port 3000 is held by a Next.js dev server (node, PID 43000) in ~/code/shop-web (branch feat/checkout), running for 2h 3m.
  → portwise stop 3000  frees it ·  portwise free-port --near 3000
```

## When it speaks up

The hook only runs after a command fails, and only for commands that start servers: `npm run dev`,
`vite`, `next dev`, `rails s`, `python manage.py runserver`, `uvicorn`, `docker run -p` and many
more. A failing `curl localhost:3000` never triggers it.

- **Explicit ports win:** `--port 4000`, `-p 4000`, `PORT=4000` or `:4000` in the command.
- **Otherwise the tool's default:** Vite 5173, Next.js 3000, Astro 4321, Angular 4200, Django and
  uvicorn 8000, Flask 5000, Storybook 6006…
- **Package-manager scripts** (`npm run dev`, `pnpm dev`) have no fixed port, so set `PORT=` or pass
  `--port` for the hook to know which one to check.

It prints nothing when the port is free, and it never changes your command's exit code.

## Remove it

Delete the line from your startup file and open a new terminal.
