/**
 * ccsm - Session Registry Pi Extension
 *
 * Integrates ccsm (Claude Code Session Manager) into Pi as native custom tools.
 * Lets Pi agents manage Claude Code sessions directly without shelling out.
 *
 * Features:
 *   - Custom tools for all common ccsm operations
 *   - Auto-injects active session context into system prompt
 *   - /ccsm command for human interaction
 */

import { spawn, execFile } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import * as os from "node:os";
import * as nodePath from "node:path";
import type { ExtensionAPI, ToolResult } from "@earendil-works/pi-coding-agent";
import { StringEnum } from "@earendil-works/pi-ai";
import { Type } from "typebox";

// ── Configuration ──────────────────────────────────────────────────

const CCSM_BIN = "ccsm";

// ── Helpers ────────────────────────────────────────────────────────

/** Run ccsm and return stdout. Throws on non-zero exit. */
function ccsm(args: string[], cwd?: string): Promise<string> {
	// Always pass --consumer pi when called from the Pi extension
	const fullArgs = ["--consumer", "pi", ...args];
	return new Promise((resolve, reject) => {
		const child = execFile(CCSM_BIN, fullArgs, {
			cwd: cwd ?? process.cwd(),
			maxBuffer: 10 * 1024 * 1024,
		});
		let stdout = "";
		let stderr = "";
		child.stdout?.on("data", (d: Buffer) => (stdout += d.toString()));
		child.stderr?.on("data", (d: Buffer) => (stderr += d.toString()));
		child.on("error", (err) => reject(new Error(`ccsm spawn failed: ${err.message}`)));
		child.on("close", (code) => {
			if (code === 0) resolve(stdout);
			else reject(new Error(`ccsm exited ${code}: ${stderr || stdout}`));
		});
	});
}

/** Run ccsm with streaming output for long-running commands (resume). */
function ccsmStream(args: string[], signal?: AbortSignal, cwd?: string): Promise<string> {
	const fullArgs = ["--consumer", "pi", ...args];
	return new Promise((resolve, reject) => {
		const child = spawn(CCSM_BIN, fullArgs, {
			cwd: cwd ?? process.cwd(),
			stdio: ["ignore", "pipe", "pipe"],
		});
		let stdout = "";
		let stderr = "";
		child.stdout.on("data", (d: Buffer) => (stdout += d.toString()));
		child.stderr.on("data", (d: Buffer) => (stderr += d.toString()));
		child.on("error", (err) => reject(new Error(`ccsm spawn failed: ${err.message}`)));
		signal?.addEventListener("abort", () => child.kill(), { once: true });
		child.on("close", (code) => {
			if (signal?.aborted) reject(new Error("aborted"));
			else if (code === 0) resolve(stdout);
			else reject(new Error(`ccsm exited ${code}: ${stderr || stdout}`));
		});
	});
}

/** Parse ccsm scan --json output. */
async function ccsmScanJson(cwd?: string): Promise<any[]> {
	const raw = await ccsm(["scan", "--json"], cwd);
	return JSON.parse(raw);
}

/** Create a text content block. */
function text(content: string): ToolResult["content"] {
	return [{ type: "text" as const, text: content }];
}

/** Derive the Pi workspace slug from the current working directory. */
function getWorkspaceSlug(): string {
	const cwd = process.cwd();
	return "--" + cwd.replace(/[^a-zA-Z0-9]/g, "-").replace(/-+/g, "-").replace(/^-|-$/g, "") + "--";
}

/** Get the UUID of the most recent Pi session in this workspace. */
function getCurrentPiSessionUuid(): string | null {
	const slug = getWorkspaceSlug();
	const dir = nodePath.join(os.homedir(), ".pi", "agent", "sessions", slug);
	if (!existsSync(dir)) return null;
	try {
		const files = readdirSync(dir)
			.filter((f) => f.endsWith(".jsonl"))
			.map((f) => ({ name: f, mtime: statSync(nodePath.join(dir, f)).mtime }))
			.sort((a, b) => b.mtime.getTime() - a.mtime.getTime());
		if (files.length === 0) return null;
		// Filename: <timestamp>_<uuid>.jsonl
		const parts = files[0].name.split("_");
		return parts.slice(1).join("_").replace(/\.jsonl$/, "");
	} catch {
		return null;
	}
}

// ── Extension ──────────────────────────────────────────────────────

export default function (pi: ExtensionAPI) {
	// ── Tool: ccsm_list ──────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_list",
		label: "CCSM List Sessions",
		description:
			"List sessions from the ccsm registry. Supports filtering by status, group, active state, and summary mode.",
		parameters: Type.Object({
			active: Type.Optional(
				Type.Boolean({ description: "Only show in_progress + blocked sessions (default: false)" }),
			),
			summary: Type.Optional(
				Type.Boolean({ description: "Show count per status only (default: false)" }),
			),
			status: Type.Optional(
				Type.String({
					description:
						"Filter by status: pending, in_progress, completed, blocked, abandoned, trashed",
				}),
			),
			group: Type.Optional(Type.String({ description: "Filter by group name" })),
			byRank: Type.Optional(
				Type.Boolean({ description: "Sort by rank within group (use with --group)" }),
			),
			verbose: Type.Optional(
				Type.Boolean({ description: "Show full goal + tags (default: false)" }),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["list"];
			if (params.active) args.push("--active");
			if (params.summary) args.push("--summary");
			if (params.status) args.push("--status", params.status);
			if (params.group) args.push("--group", params.group);
			if (params.byRank) args.push("--by-rank");
			if (params.verbose) args.push("--verbose");
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "list" } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_scan ──────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_scan",
		label: "CCSM Scan Sessions",
		description:
			"Compact scan-friendly output of sessions. Supports full-text search across name, goal, and tags. Grep-friendly format.",
		parameters: Type.Object({
			search: Type.Optional(
				Type.String({
					description: "Full-text search across name, goal, and tags (case-insensitive)",
				}),
			),
			group: Type.Optional(Type.String({ description: "Filter by group name" })),
			status: Type.Optional(Type.String({ description: "Filter by status" })),
			json: Type.Optional(
				Type.Boolean({ description: "Output as JSON array for programmatic use" }),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["scan"];
			if (params.search) args.push("--search", params.search);
			if (params.group) args.push("--group", params.group);
			if (params.status) args.push("--status", params.status);
			if (params.json) args.push("--json");
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "scan" } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_show ──────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_show",
		label: "CCSM Show Session",
		description: "Show full details for a session: goal, scope, tags, session_id, pids, timestamps.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
			section: Type.Optional(
				Type.String({
					description: "Extract one section (e.g. 'progress-log', 'checklist', 'group')",
				}),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["show", params.name];
			if (params.section) args.push("--section", params.section);
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "show", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_new ───────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_new",
		label: "CCSM New Session",
		description:
			"Create a new pending session entry. Name must be kebab-case. Goal should be a keyword-rich one-sentence description.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case, e.g. 'add-dark-mode')" }),
			goal: Type.String({
				description: "One-sentence goal describing what this session accomplishes",
			}),
			checklist: Type.Optional(
				Type.Boolean({ description: "Also create a ## Checklist section in the detail file" }),
			),
			force: Type.Optional(
				Type.Boolean({ description: "Skip fuzzy duplicate detection" }),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["new", params.name, "-g", params.goal];
			if (params.checklist) args.push("-c");
			if (params.force) args.push("-f");
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "new", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_start ─────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_start",
		label: "CCSM Start Session",
		description: "Transition a session from pending to in_progress.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm(["start", params.name]);
				return { content: text(output), details: { command: "start", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_complete ──────────────────────────────────────
	pi.registerTool({
		name: "ccsm_complete",
		label: "CCSM Complete Session",
		description:
			"Transition a session from in_progress to completed. Sets completed timestamp. Run close gate first with ccsm_close.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
			force: Type.Optional(
				Type.Boolean({ description: "Skip gate checks (detail file completeness etc.)" }),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["complete", params.name];
			if (params.force) args.push("--force");
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "complete", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_block ─────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_block",
		label: "CCSM Block Session",
		description: "Mark a session as blocked (waiting on dependency).",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm(["block", params.name]);
				return { content: text(output), details: { command: "block", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_abandon ───────────────────────────────────────
	pi.registerTool({
		name: "ccsm_abandon",
		label: "CCSM Abandon Session",
		description: "Mark a session as abandoned (no longer relevant).",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm(["abandon", params.name]);
				return { content: text(output), details: { command: "abandon", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_scope ─────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_scope",
		label: "CCSM Set Scope",
		description:
			"Set the scope for a session: 2-4 sentences on approach, constraints, what's in/out.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
			text: Type.String({ description: "Scope text: approach, constraints, what's in/out" }),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm(["scope", params.name, params.text]);
				return { content: text(output), details: { command: "scope", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_tag ───────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_tag",
		label: "CCSM Set Tags",
		description: "Replace tags on a session. Tags are lowercase, single words or hyphenated.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
			tags: Type.String({
				description: "Space-separated tags, e.g. 'ux frontend dark-mode'",
			}),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm(["tag", params.name, ...params.tags.split(/\s+/)]);
				return { content: text(output), details: { command: "tag", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_note ──────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_note",
		label: "CCSM Add Note",
		description:
			"Append a timestamped progress note to a session's detail file. Use for recording decisions, blockers, and key findings.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
			text: Type.String({ description: "Note text — what happened, decided, or was learned" }),
			cross: Type.Optional(
				Type.String({
					description:
						"Cross-session note source: prepends 'CROSS-SESSION [source]: ' to the note",
				}),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["note", params.name, params.text];
			if (params.cross) args.push("--cross", params.cross);
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "note", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_check ─────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_check",
		label: "CCSM Checklist Item",
		description:
			"Add or update a checklist item. If no existing item matches by number or text, a new item is added. Status options: pending, done, skipped, blocked.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
			item: Type.String({
				description: "1-based number (1, 2, ...), text substring, or new item text",
			}),
			status: StringEnum(["pending", "done", "skipped", "blocked"] as const),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm([
					"check",
					params.name,
					params.item,
					"-s",
					params.status,
				]);
				return { content: text(output), details: { command: "check", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_next ──────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_next",
		label: "CCSM Next Session",
		description:
			"Print the next session to work on in a group. Priority: in_progress > pending by rank (numeric: lowest first, free: alphabetical). Exits 0 with no output if all done.",
		parameters: Type.Object({
			group: Type.String({ description: "Group name to get next session from" }),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm(["next", params.group]);
				return { content: text(output), details: { command: "next", group: params.group } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_inject_scope ──────────────────────────────────
	pi.registerTool({
		name: "ccsm_inject_scope",
		label: "CCSM Inject Scope",
		description:
			"Output the active session's goal and scope as a system-reminder block. Pass a session name or auto-detect the in_progress session.",
		parameters: Type.Object({
			name: Type.Optional(
				Type.String({
					description:
						"Session name (kebab-case). Omit to auto-detect the in_progress session.",
				}),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["inject-scope"];
			if (params.name) args.push(params.name);
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "inject-scope", name: params.name ?? "(auto)" } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_close ─────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_close",
		label: "CCSM Close Gate",
		description:
			"Pre-completion gate: check detail file completeness, print self-review checklist. Run before ccsm_complete. Exits non-zero if the detail file is hollow.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm(["close", params.name]);
				return { content: text(output), details: { command: "close", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_resume ────────────────────────────────────────
	//
	// When called from within Pi, this attaches the current Pi session UUID
	// to the ccsm session rather than spawning a new Pi process.
	//
	// To spawn a new Pi process for the session, use from a terminal:
	//   ccsm --consumer pi resume <name>
	pi.registerTool({
		name: "ccsm_resume",
		label: "CCSM Resume Session",
		description:
			"Link the current Pi session to a ccsm session. When called from within Pi, attaches the current Pi session UUID. From a terminal, spawns a new Pi process.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				// Auto-attach current Pi session UUID
				const piUuid = getCurrentPiSessionUuid();
				if (piUuid) {
					await ccsm(["attach", params.name, piUuid]);
					await ccsm(["start", params.name]);
					const showOutput = await ccsm(["inject-scope", params.name]);
					return {
						content: text(`Linked Pi session ${piUuid.slice(0, 8)} to ccsm session '${params.name}'.\n\n${showOutput}`),
						details: { command: "resume", name: params.name, action: "attach" },
					};
				}
				return {
					content: text("Could not find current Pi session UUID. To spawn a new Pi: ccsm --consumer pi resume <name> from a terminal"),
					details: { command: "resume", name: params.name, error: "no pi uuid" },
				};
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_doctor ────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_doctor",
		label: "CCSM Doctor",
		description: "Scan for health issues: orphaned IDs, dead PIDs, empty fields, cleanup candidates.",
		parameters: Type.Object({}),
		async execute(_id, _params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm(["doctor"]);
				return { content: text(output), details: { command: "doctor" } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_pending ───────────────────────────────────────
	pi.registerTool({
		name: "ccsm_pending",
		label: "CCSM Reset to Pending",
		description:
			"Reset a session to pending status. Clears session_id, pids, and timestamps. Use when re-opening a completed/abandoned session.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			try {
				const output = await ccsm(["pending", params.name]);
				return { content: text(output), details: { command: "pending", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_group ─────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_group",
		label: "CCSM Session Group",
		description:
			"Manage session groups. Assign a session to a group (with optional rank), remove from group, or view group overview.",
		parameters: Type.Object({
			session: Type.String({ description: "Session name (kebab-case)" }),
			group: Type.Optional(Type.String({ description: "Group name to assign session to" })),
			rank: Type.Optional(
				Type.String({
					description: "Rank: 'free' (any order) or a number (lower = higher priority)",
				}),
			),
			clear: Type.Optional(
				Type.Boolean({ description: "Remove session from its group" }),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["group", params.session];
			if (params.clear) {
				args.push("--clear");
			} else if (params.group) {
				args.push("--group", params.group);
				if (params.rank) args.push("--rank", params.rank);
			}
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "group", session: params.session } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_depend ────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_depend",
		label: "CCSM Dependencies",
		description: "Manage session dependencies. Add a dependency, list dependencies, or clear all.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
			on: Type.Optional(
				Type.String({ description: "Add a dependency: session must complete first" }),
			),
			clear: Type.Optional(
				Type.Boolean({ description: "Remove all dependencies" }),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["depend", params.name];
			if (params.clear) args.push("--clear");
			else if (params.on) args.push("--on", params.on);
			try {
				const output = await ccsm(args);
				return {
					content: text(output),
					details: { command: "depend", name: params.name },
				};
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_attach ────────────────────────────────────────
	pi.registerTool({
		name: "ccsm_attach",
		label: "CCSM Attach Session",
		description:
			"Manually link a Claude session_id to a ccsm session. Can use session UUID or PID to harvest from live session file.",
		parameters: Type.Object({
			name: Type.String({ description: "Session name (kebab-case)" }),
			session_id: Type.Optional(
				Type.String({ description: "Session UUID (from ~/.claude/sessions/<pid>.json)" }),
			),
			pid: Type.Optional(
				Type.Number({ description: "PID to harvest session_id from live session file" }),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["attach", params.name];
			if (params.session_id) args.push(params.session_id);
			if (params.pid) args.push("--pid", String(params.pid));
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "attach", name: params.name } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_gate_check ────────────────────────────────────
	pi.registerTool({
		name: "ccsm_gate_check",
		label: "CCSM Gate Check",
		description:
			"Check if current work aligns with session scope. Exit 0 = pass, 1 = fail. Designed for stop hooks before ccsm_complete.",
		parameters: Type.Object({
			name: Type.Optional(
				Type.String({
					description: "Session name (auto-detects in_progress if omitted)",
				}),
			),
			strict: Type.Optional(
				Type.Boolean({ description: "Strict mode: fail if scope is empty or unfilled" }),
			),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const args: string[] = ["gate-check"];
			if (params.name) args.push(params.name);
			if (params.strict) args.push("--strict");
			try {
				const output = await ccsm(args);
				return {
					content: text(output),
					details: { command: "gate-check", name: params.name ?? "(auto)" },
				};
			} catch (err: any) {
				return { content: text(`Gate check failed: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Tool: ccsm_sequence ──────────────────────────────────────
	pi.registerTool({
		name: "ccsm_sequence",
		label: "CCSM Batch Operations",
		description:
			"Run multiple ccsm mutations in a single lock/load/save cycle. Each operation is prefixed with -q. Example: -q start foo -q scope foo 'text' -q complete foo",
		parameters: Type.Object({
			operations: Type.String({
				description:
					"Space-separated sequence of operations. Each operation group starts with -q. e.g. '-q start foo -q complete foo'",
			}),
		}),
		async execute(_id, params, _signal, _onUpdate, _ctx) {
			const parts = params.operations.trim().split(/\s+/);
			const args = ["sequence", ...parts];
			try {
				const output = await ccsm(args);
				return { content: text(output), details: { command: "sequence" } };
			} catch (err: any) {
				return { content: text(`Error: ${err.message}`), details: { error: err.message } };
			}
		},
	});

	// ── Session Context Injection + Auto-Attach ──────────────────
	//
	// On each agent start:
	// 1. Auto-attach this Pi session UUID to the in_progress ccsm session
	//    (if not already linked)
	// 2. Inject the active session's goal and scope into the system prompt

	pi.on("before_agent_start", async (_event, _ctx) => {
		let activeSessionName: string | null = null;

		try {
			// Find the in_progress ccsm session (if any)
			const listOutput = await ccsm(["list", "--active", "--json"]);
			const sessions = JSON.parse(listOutput);
			const active = sessions.find((s: any) => s.status === "in_progress" || s.status === "blocked");
			if (active) {
				activeSessionName = active.name;
			}
		} catch {
			// swallow
		}

		// Auto-attach current Pi session UUID to the active ccsm session
		if (activeSessionName) {
			const piUuid = getCurrentPiSessionUuid();
			if (piUuid) {
				try {
					// Check if already attached
					const showOutput = await ccsm(["show", activeSessionName]);
					if (!showOutput.includes(piUuid)) {
						await ccsm(["attach", activeSessionName, piUuid]);
					}
				} catch {
					// swallow — attach is best-effort
				}
			}
		}

		// Inject goal + scope into system prompt
		if (activeSessionName) {
			try {
				const output = await ccsm(["inject-scope", activeSessionName]);
				if (output) {
					return { systemPrompt: output.trim() };
				}
			} catch {
				// swallow
			}
		}
		return { systemPrompt: "" };
	});

	// ── /ccsm Command ────────────────────────────────────────────
	//
	// Human-facing command for quick ccsm interaction.

	pi.registerCommand("ccsm", {
		description:
			"Run ccsm commands interactively. Usage: /ccsm <subcommand> [args...]",
		handler: async (args, ctx) => {
			if (!args.trim()) {
				ctx.ui.notify(
					"Usage: /ccsm <subcommand> [args...] — e.g. /ccsm list, /ccsm scan",
					"info",
				);
				return;
			}
			try {
				const parts = args.trim().split(/\s+/);
				const output = await ccsm(parts);
				ctx.ui.notify(output.substring(0, 500), "info");
			} catch (err: any) {
				ctx.ui.notify(`ccsm error: ${err.message}`, "error");
			}
		},
	});
}
