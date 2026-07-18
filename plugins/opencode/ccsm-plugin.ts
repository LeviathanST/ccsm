import type { Plugin } from "@opencode-ai/plugin";

const HOME = process.env.HOME || "/tmp";
const SWARM_DB = `${HOME}/.ccsm/swarm.db`;

async function run(cmd: string): Promise<string> {
  const proc = Bun.spawn(["sh", "-c", cmd], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const out = await new Response(proc.stdout).text();
  await proc.exited;
  return out.trim();
}

function ccsmRegistryPath(): string | null {
  const cwd = process.cwd();
  const walk = (dir: string, depth: number): string | null => {
    if (depth > 10) return null;
    try {
      const raw = require("fs").readFileSync(`${dir}/.ccsm`, "utf-8");
      const match = raw.match(/id\s*=\s*"([^"]+)"/);
      if (match) return `${HOME}/.ccsm/${match[1]}/sessions.json`;
    } catch {}
    const parent = dir.split("/").slice(0, -1).join("/") || "/";
    if (parent === dir) return null;
    return walk(parent, depth + 1);
  };
  return walk(cwd, 0);
}

function isOrchestrator(name: string): boolean {
  const regPath = ccsmRegistryPath();
  if (!regPath) return false;
  try {
    const reg = JSON.parse(require("fs").readFileSync(regPath, "utf-8"));
    return reg.sessions?.some(
      (s: any) => s.name === name && s.is_orchestrator === true
    ) ?? false;
  } catch {
    return false;
  }
}

async function findOrchestrator(workerSid: string): Promise<string | null> {
  try {
    const result = await run(
      `sqlite3 "${SWARM_DB}" "SELECT orch_sid, worker_name FROM workers WHERE worker_sid='${workerSid}' LIMIT 1"`
    );
    if (result) {
      const [orchSid, workerName] = result.split("|");
      if (orchSid) return orchSid;
    }
  } catch {}
  return null;
}

export const CcsmPlugin: Plugin = async ({ $ }) => {
  const sessionName = process.env.CCSM_SESSION;

  const orch = sessionName ? isOrchestrator(sessionName) : false;

  return {
    event: async ({ event }) => {
      // ── session.created: auto-attach ccsm session ID ─────────
      if (event.type === "session.created") {
        const sid = event.properties?.info?.id;
        const dir = event.properties?.info?.directory;
        if (!sid || !dir) return;
        // wait briefly for ccsm start to complete
        await new Promise((r) => setTimeout(r, 2000));
        try {
          await $`ccsm attach ${sessionName || ""} ${sid}`.quiet();
        } catch {}
      }

      // ── session.idle: auto-notify orchestrator ───────────────
      if (event.type === "session.idle") {
        const workerSid = event.properties?.info?.id;
        if (!workerSid) return;
        // Don't notify if THIS session is the orchestrator
        if (orch) return;
        const orchSid = await findOrchestrator(workerSid);
        if (!orchSid) return;

        // Check orchestrator is alive
        try {
          const pwFile = require("fs")
            .readdirSync(`${HOME}/.config/opencode`)
            .find((f: string) => f.startsWith("service-") && f.endsWith(".json"));
          const pw = pwFile
            ? JSON.parse(
                require("fs").readFileSync(
                  `${HOME}/.config/opencode/${pwFile}`,
                  "utf-8"
                )
              ).password
            : "";

          const auth = Buffer.from(`opencode:${pw}`).toString("base64");
          const checkResp = await fetch(
            `http://127.0.0.1:4096/api/session/${orchSid}`,
            { headers: { Authorization: `Basic ${auth}` } }
          );
          if (!checkResp.ok) return; // orchestrator not alive

          // Send notification
          await fetch(
            `http://127.0.0.1:4096/api/session/${orchSid}/prompt`,
            {
              method: "POST",
              headers: {
                "Content-Type": "application/json",
                Authorization: `Basic ${auth}`,
              },
              body: JSON.stringify({
                text: `Worker completed (session ${workerSid})`,
              }),
            }
          );
        } catch {}
      }
    },

    "experimental.chat.system.transform": async (input, output) => {
      if (!input.sessionID) return;
      const name = sessionName;
      if (!name) return;

      // Inject ccsm context
      try {
        const { stdout } = await $`ccsm inject-scope ${name}`.quiet();
        if (stdout) {
          output.system.push(stdout.toString());
        }
      } catch {}

      // ── Orchestrator persona ──
      if (orch) {
        const regPath = ccsmRegistryPath();
        let workers = "";
        if (regPath) {
          try {
            const reg = JSON.parse(
              require("fs").readFileSync(regPath, "utf-8")
            );
            const session = reg.sessions?.find(
              (s: any) => s.name === name
            );
            workers = (session?.tags ?? [])
              .filter((t: string) => t.startsWith("worker:"))
              .map((t: string) => t.replace("worker:", ""))
              .join(", ");
          } catch {}
        }

        output.system.push(
          `You are an orchestrator agent. Your workers: ${
            workers || "(check detail file)"
          }. ` +
            `Use swarm-spawn to start them all at once (non-blocking, parallel). ` +
            `Provide a clear task prompt for each worker. ` +
            `Workers read their own ccsm session detail to understand their goal. ` +
            `Use swarm-status to check progress. ` +
            `Workers will auto-notify you when they complete. ` +
            `When all workers are done, review their output and report to the user.`
        );
      }
    },

    "experimental.session.compacting": async (input, output) => {
      const name = sessionName;
      if (!name) return;
      output.context.push(`This session is managed by ccsm: ${name}`);
      try {
        const { stdout } = await $`ccsm inject-scope ${name}`.quiet();
        if (stdout) {
          output.context.push(stdout.toString());
        }
      } catch {}
    },
  };
};
