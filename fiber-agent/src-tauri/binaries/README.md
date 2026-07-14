# Bundled FNN sidecar

`tauri.conf.json` embeds `binaries/fnn` via `externalBin`.

Tauri expects a **target-triple** filename next to this folder:

| Platform | File |
|----------|------|
| Windows x64 | `fnn-x86_64-pc-windows-msvc.exe` |

From `fiber-agent/`:

```powershell
npm run prepare:fnn-sidecar
```

That copies `../fnn-testnet/fnn.exe` into the correct name. Then:

```powershell
npm run tauri:build
```
