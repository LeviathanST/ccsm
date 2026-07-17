import { Plugin } from "@opencode-ai/plugin/v2"
import { execFileSync } from "child_process";
import { readFileSync, existsSync, readdirSync } from "fs";
import { join } from "path";

function workspaceIdFromDir(dir: string): string | null {
  let cur = dir;
  for (let i = 0; i < 10; i++) {
    try {
      const raw = readFileSync(join(cur, ".ccsm"), "utf-8");
      const match = raw.match(/id\s*=\s*"([^"]+)"/);
      if (match) return match[1];
    } catch {}
    const parent = join(cur, "..");
    if (parent === cur) break;
    cur = parent;
  }
  return null;
}

function findInRegistry(
  workspaceId: string,
  matcher: (s: any) => boolean
): { name: string } | null {
  const regPath = join(process.env.HOME || "/tmp", ".ccsm", workspaceId, "sessions.json");
  if (!existsSync(regPath)) return null;
  try {
    const reg = JSON.parse(readFileSync(regPath, "utf-8"));
    for (const s of reg.sessions || []) {
      if (matcher(s)) return { name: s.name };
    }
  } catch {}
  return null;
}

export default Plugin.define({
  id: "ccsm",
  setup: async (ctx) => {
    (async () => {
      for await (const event of ctx.event.subscribe()) {
        if (event.type === "session.created") {
          const dir = event.data.info.directory;
          const sessionId = event.data.sessionID;
          const wsId = workspaceIdFromDir(dir);
          if (!wsId) continue;
          const sessionName = dir.split("/").pop() || "";
          const found = findInRegistry(wsId, (s) =>
            s.status === "in_progress" && !s.session_id && s.name === sessionName
          );
          if (found) {
            try {
              execFileSync("ccsm", ["attach", found.name, sessionId], {
                timeout: 5000, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
                cwd: dir,
              });
            } catch {}
          }
        }
      }
    })();

    await ctx.session.hook("context", async (ctx2) => {
      let sessionDir: string | null = null;
      try {
        const info = await ctx.session.get({ sessionID: ctx2.sessionID });
        sessionDir = info.location?.directory || null;
      } catch {}

      const ccsmDir = join(process.env.HOME || "/tmp", ".ccsm");
      let found: { name: string } | null = null;
      let wsId: string | null = null;

      if (sessionDir) {
        wsId = workspaceIdFromDir(sessionDir);
      }
      if (wsId) {
        found = findInRegistry(wsId, (s) => s.session_id === ctx2.sessionID);
      }
      if (!found && existsSync(ccsmDir)) {
        for (const entry of readdirSync(ccsmDir)) {
          found = findInRegistry(entry, (s) => s.session_id === ctx2.sessionID);
          if (found) { wsId = entry; break; }
        }
      }
      if (!found && sessionDir) {
        const sessionName = sessionDir.split("/").pop() || "";
        if (wsId) {
          found = findInRegistry(wsId, (s) => s.name === sessionName);
        }
      }
      if (!found || !wsId) return;

      const cwd = sessionDir || join(ccsmDir, wsId);
      try {
        const stdout = execFileSync("ccsm", ["inject-scope", found.name], {
          timeout: 5000, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
          cwd,
        });
        if (stdout) {
          ctx2.system.push({ type: "text", text: stdout });
        }
      } catch {}
    });
  },
});
