# Swarm session.idle notification loop

**Symptom:** After implementing session.idle auto-notify in plugin, completed workers cause infinite notification loop — orchestrator receives the same "Worker X finished" message repeatedly, once per agent turn.

**Cause:** The notification fires on every session.idle event without tracking which workers have already been reported. The idle event re-triggers when the session becomes idle again (after processing the notification prompt). Plugin has no deduplication — it checks `swarm.db` on every idle event and sends the same notification.

**Fix:** Track notified workers in `swarm.db` with a `notified` boolean column. Plugin sets it to true after first notification and skips workers already notified. Alternatively, clear the `worker_sid` after notification so the lookup returns null.

**Evidence:** 15+ repeated notifications in orchestrator session for workers `ccsm-swarm-kill-cli`, `ccsm-swarm-plugin`, `ccsm-swarm-skill-tests`. Each notification triggered a new agent turn, which completed, causing another idle event, restarting the cycle.
