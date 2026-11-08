# Auto-save (Zsh)

Install the current build and reload the hook after changes to its template:

```sh
cargo install --path . --force
eval "$(ark init zsh)"
ark auto on
```

Keep `eval "$(ark init zsh)"` in `~/.zshrc` for new terminals. Use
`ark auto on --confirm` to ask before saving, or `ark auto off` to disable it.

The same Zsh integration provides dynamic completion from Ark's local registry:
type `ark <Tab>` to complete a saved alias, or use Tab after `rm`, `star`,
`unstar`, `update`, or `fwd`. Your Zsh completion system must be initialized
first (for example, `autoload -Uz compinit && compinit` in `.zshrc`); Ark does
not run `compinit` itself.

## Detection and retries

- Direct interactive `codex` and `claude` commands create a per-shell pending
  record. Headless commands such as `codex exec` and `claude -p` are ignored.
- An ordinary prompt without a pending file does not launch Ark at all.
- After exit, discovery examines Codex's date directories covering the command
  (plus adjacent dates for timezone boundaries), or Claude's matching project
  directory. It does not recursively scan the whole history.
- Only transcripts created during that command, with the matching working
  directory, qualify. This path captures **new sessions**, not existing sessions
  resumed from an earlier invocation. Use `ark add <session-id>` for those.
- Discovery reads at most the first 1 MiB of each changed candidate. A partial
  final JSONL record is tolerated. Cached metadata is reused while file size and
  modification time remain unchanged. Very late titles/prompts may be omitted.
- A miss retains the pending record. Later prompts retry with 2, 5, 10, then
  30-second backoff, for up to five minutes **after exit**. There are no sleeps or
  background workers: waiting at an idle prompt alone does not trigger a retry.
  Press Enter after a couple of seconds to trigger another prompt.
- A successful add prints `Ark: saved ...`. Expiry or ambiguity prints a reason
  and asks for manual `ark add`; existing registry entries are not duplicated.

Provider paths respect `CODEX_HOME` and `CLAUDE_CONFIG_DIR`. Otherwise they use
`~/.codex` and `~/.claude`. Wrapped/aliased commands, commands that change their
own working directory, unusual provider storage layouts, and resumed sessions
are not automatically resolved by this hook.

## Attribution limits

Multiple candidates are never resolved by choosing the newest. Overlapping
hooked commands using the same provider and directory are conservatively marked
ambiguous. This is best-effort, not proof of process ownership: unhooked terminals
and concurrent hook races cannot be ruled out from transcript timestamps alone.
Provider-native session IDs/events would be needed for guaranteed attribution.

## Diagnostics

Enable temporary diagnostics before launching a provider:

```sh
export ARK_AUTO_DEBUG=1
codex
# Exit Codex. If discovery misses, press Enter after a few seconds.
ark ls
unset ARK_AUTO_DEBUG
```

Diagnostics report candidate counts, changed-file parse counts, and retry
reasons. Per-shell retry state lives in `~/.ark/pending/$$.json`; it is removed
after success, an explicit skip, ambiguity, expiry, or disabling auto mode.
Provider transcript files are read-only. Tests use isolated temporary stores and
do not launch providers or touch your real registry.
