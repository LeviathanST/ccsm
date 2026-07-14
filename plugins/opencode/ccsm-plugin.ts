import type { Plugin } from "@opencode-ai/plugin";

export const CcsmPlugin: Plugin = async ({ $, directory }) => {
  const sessionName = process.env.CCSM_SESSION;
  if (!sessionName) return {};

  return {
    "experimental.chat.system.transform": async (_input, output) => {
      const { stdout } = await $`ccsm inject-scope`.quiet();
      if (stdout) {
        output.system.push(stdout.toString());
      }
    },

    event: async ({ event }) => {
      if (event.type === "session.created") {
        // Link opencode session to ccsm registry
        await $`ccsm attach ${sessionName} ${event.sessionID}`.quiet();
      }
    },
  };
};
