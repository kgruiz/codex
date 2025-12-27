# Background: Shell Commands Continue After Cancel

## Summary
Users can cancel a running Codex turn, but shell commands started via the
user shell path can continue running in the background. This shows up as
long-lived processes (for example, `cargo install` or `just` invocations)
still holding build locks even after the UI reports the response was stopped.
The user experience is confusing and can block subsequent commands.

## Observed Symptoms
- After canceling a response, `just id` (which runs `cargo install ...`)
  continues running, and later attempts hang on a build lock.
- `ps` shows the command still active even though the turn is aborted.
- The UI does not show an explicit "aborted by user" command end event for
  the in-flight command.

## Current Behavior (Where It Goes Wrong)
- The session cancellation token is triggered when the user stops a turn.
- The user shell task returns early on cancellation, but the underlying
  process is not terminated.
- The exec layer only kills the child process group when its own expiration
  mechanism fires; if the task is cancelled upstream without propagating that
  cancellation into the exec call, the process continues.

## Root Cause Hypothesis
Cancellation is being handled at the task/future level rather than being
wired into the exec process. This means the task stops awaiting, but the child
process group is never sent a kill signal. The process continues to run and
retains its file locks.

## Impact
- Background processes can keep running after the user cancels.
- File locks (e.g., Cargo build locks) block future commands.
- Users perceive cancel as unreliable and may lose confidence in stopping
  long-running commands.

## Constraints / Requirements
- Do not change or reference CODEX sandbox env vars.
- Preserve existing exec output formatting and event emission.
- Ensure cancellation is consistent with timeout behavior (kill process group).
- Keep user-facing messaging stable and clear ("aborted by user").

## Plan for Fix
1. Propagate cancellation into the exec layer for user shell commands.
   - Create a dedicated `CancellationToken` for the exec invocation.
   - Cancel it when either the user cancels the turn or the 1h user-shell
     timeout elapses.
2. Use the exec expiration mechanism instead of cancelling the future:
   - Pass `ExecExpiration::Cancellation(token)` into `ExecEnv`.
   - Rely on `execute_exec_env` to kill the process group when the token
     fires.
3. Handle the cancellation result explicitly:
   - If the exec returns a timeout error AND the user cancellation token is
     cancelled, treat this as a user abort (exit code -1, message "aborted by
     user").
   - Ensure `ExecCommandEnd` is emitted with the aborted message.
4. Confirm that stdout/stderr collection does not hang:
   - Continue to rely on existing drain timeouts in exec to avoid pipe hangs.
5. Verification
   - Start a long-running shell command, cancel the turn, and confirm the
     process is gone in `ps`.
   - Ensure no lingering `cargo install` process holds the lock.
   - Verify a user-cancel produces a clean `ExecCommandEnd` event with
     "aborted by user".

## Risks / Mitigations
- Risk: treating all cancellations as timeouts could hide real timeouts.
  - Mitigation: only classify as user-aborted when the session cancellation
    token is explicitly cancelled; otherwise keep timeout behavior.
- Risk: killing process groups could terminate unrelated processes if the
  child spawns a shared group.
  - Mitigation: continue using existing process-group kill logic and only
    apply it to the spawned process group for the exec call.
