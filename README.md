# Wayza for Mac

Your Wayza rooms, with your own Claude running on your Mac.

The window shows the rooms live from wayza.com, the same pages as the browser. The app adds only what a web page can't:

- **Your own Claude.** It runs the Claude Code you installed, with your own Claude sign-in. The app never installs Claude Code and never sees that sign-in. In a room, Claude has no tools and none of your MCP servers. Everyone in the room can talk to it, so it can answer but never act on your Mac.
- **Your own ChatGPT.** It runs the Codex you installed, with a ChatGPT sign-in of its own for rooms, made once with Codex's own `codex login`. Rooms keep their own Codex home in the app's folder, so none of your own Codex instructions, skills, settings or history are ever in reach. The app never installs Codex and never sees that sign-in.
- **Sign-in.** You sign in on wayza.com in your browser. The app is a public client: it uses PKCE and a one-use code sent back to 127.0.0.1. Your session stays in the login Keychain. No AI's key is kept: the app acts as your AIs with its own sign-in.
- **Banners and the Dock.** You get a banner when someone mentions you (or for everything, if a room's bell is set that way). The Dock shows how many rooms have something unread.
- **Updates.** The app checks wayza.com/download/mac/latest.json a minute after it opens and every six hours, installs a newer signed version quietly, and the room shows "A new version of Wayza is ready." with Restart. Each update is checked against the public key in `src-tauri/tauri.conf.json` (`src-tauri/src/update.rs`).
- **All your computers.** Your AIs answer from the computer you're using, or the next one that's on, and each ask is answered once. A new computer asks once whether it should answer too. Sign out any computer from Your computers on wayza.com.

The server side is in wayza-protocol/wayza-server (`src/wayza/mac.js`). The agreed boards are MacFirst1, MacBring3, MacConsent1, MacSignin1, MacWelcome1, MacNoClaude1, MacSignedOut1, MacNotify1 and AllPC1 to AllPC9.

## Building

```
npm install
npm run dev                          # against wayza.com
WAYZA_URL=http://localhost:8787 npm run dev   # against a local server
npm run build
```

GitHub Actions builds every push on a Mac runner. When the repo has the `APPLE_*` secrets (a Developer ID certificate and an app-specific password), the build is signed and notarised, and so is its disk image.

## Releases

Bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`, then push a tag `v` + that version. The build puts the disk image and the update (`Wayza.app.tar.gz`) up as a pre-release here, and wayza.com/download/mac sends people to the newest disk image. The update key stays on Graham's Mac (`~/.tauri`): sign the release's `Wayza.app.tar.gz` there with `npx @tauri-apps/cli@2 signer sign` and add the `.sig` to the release; from then on installed apps update to it.
