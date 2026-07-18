import type { Plugin } from "@opencode-ai/plugin";
import { writeFileSync, mkdirSync, existsSync, readFileSync, unlinkSync } from "fs";
import { join } from "path";

function sidecarDir() {
  return join(process.env.HOME || "/tmp", ".ccsm", "swarm-context");
}

function sidecarPath(sessionID: string) {
  return join(sidecarDir(), `${sessionID}.json`);
}

function readSidecar(sessionID: string) {
  const path = sidecarPath(sessionID);
  if (!existsSync(path)) return null;
  return JSON.parse(readFileSync(path, "utf-8"));
}

export const CcsmPlugin: Plugin = async ({ $ }) => {
  const sessionName = process.env.CCSM_SESSION;

  return {
    event: async ({ event }) => {
      if (event.type === "session.created") {
        const dir = sidecarDir();
        if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
        writeFileSync(join(dir, `${event.properties.info.id}.json`), JSON.stringify({
          session_id: event.properties.info.id,
          ccsm_name: event.properties.info.title,
          title: event.properties.info.title,
          directory: event.properties.info.directory,
          timestamp: Date.now(),
        }));
      }
      if (event.type === "session.deleted") {
        const path = sidecarPath(event.properties.info.id);
        if (existsSync(path)) unlinkSync(path);
      }
    },

    "experimental.chat.system.transform": async (input, output) => {
      if (!input.sessionID) return;
      const data = readSidecar(input.sessionID);
      const name = data?.ccsm_name || sessionName;
      if (!name) return;
      try {
        const { stdout } = await $`ccsm inject-scope ${name}`.quiet();
        if (stdout) {
          output.system.push(stdout.toString());
        }
      } catch {
        // inject-scope failed — session may not exist in ccsm yet
      }
    },

    "experimental.session.compacting": async (input, output) => {
      const data = readSidecar(input.sessionID);
      if (!data) return;
      const name = data?.ccsm_name || sessionName;
      if (!name) return;
      output.context.push(`This session is managed by ccsm: ${data.title}`);
      try {
        const { stdout } = await $`ccsm inject-scope ${name}`.quiet();
        if (stdout) {
          output.context.push(stdout.toString());
        }
      } catch {
        // inject-scope failed — session may not exist in ccsm yet
      }
    },
  };
};
