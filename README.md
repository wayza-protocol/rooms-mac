# Wayza for Mac

Your Wayza rooms, with your own Claude running on your Mac.

The window shows the rooms live from wayza.com, the same pages as the browser. The app adds only what a web page can't:

- **Your own Claude.** It runs the Claude Code you installed, with your own Claude sign-in. The app never installs Claude Code and never sees that sign-in. In a room, Claude has no tools and none of your MCP servers. Everyone in the room can talk to it, so it can answer but never act on your Mac.
- **Your own ChatGPT.** It runs the Codex you installed, with a ChatGPT sign-in of its own for rooms, made once with Codex's own `codex login`. Rooms keep their own Codex home in the app's folder, so none of your own Codex instructions, skills, settings or history are ever in reach. The app never installs Codex and never sees that sign-in.
- **Sign-in.** You sign in on wayza.com in your browser. The app is a public client: it uses PKCE and a one-use code sent back to 127.0.0.1. Your session and your Claude's key stay in the login Keychain.
- **Banners and the Dock.** You get a banner when someone mentions you (or for everything, if a room's bell is set that way). The Dock shows how many rooms have something unread.
- **Use this Mac.** Your Claude runs on one Mac at a time.

The server side is in wayza-protocol/wayza-server (`src/wayza/mac.js`). The agreed boards are MacFirst1, MacBring3, MacConsent1, MacSignin1, MacWelcome1, MacNoClaude1, MacSignedOut1, MacAway2 and MacNotify1.

## Building

```
npm install
npm run dev                          # against wayza.com
WAYZA_URL=http://localhost:8787 npm run dev   # against a local server
npm run build
```

GitHub Actions builds every push on a Mac runner. When the repo has the `APPLE_*` secrets (a Developer ID certificate and an app-specific password), the build is signed and notarised.
