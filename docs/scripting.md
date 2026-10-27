# Using OTPKeep from scripts

[简体中文](scripting.zh-CN.md)

`otpkeep code` prints the current code of one account, so shell scripts, Python, Node.js, launchers and
automation tools can fill in two-factor codes. This page is the reference for that interface.

## When it works

| Situation | What `otpkeep code` does |
|---|---|
| The OTPKeep app is **not** running | Opens the vault itself. With a master password it reads it from the terminal or from `--password-stdin`. |
| The app is running and unlocked (macOS, Linux) | Asks the app through a private local socket and prints its answer at once. |
| The app is running but waits for its master password | Opens the vault itself (the app does not hold it while locked). |
| The app is running and **scripts are not allowed** | Fails at once with `DISABLED` (exit code 12). |
| The app is running on Windows | Not supported yet: the command waits up to 5 seconds for the vault and fails with `BUSY` (exit code 6). Quit the app, or use the app itself. |

The app answers only when **Settings → Allow scripts to get codes from the running app** is on:

- **On by default** when the vault's key is kept in the system keychain (no master password). The command line
  could open the vault directly in that case anyway, so the setting adds no new access.
- **Off by default** when the vault has a master password. Turning it on lets any program you run get codes
  while the app is unlocked, without the password. Turn it on only if that is what you want.

Codes given to scripts do **not** count as "recently used": the menu bar menu keeps showing the accounts you
use yourself.

## Quick start

```sh
otpkeep code github                 # prints 492081
otpkeep code github --json          # prints {"data":{"code":"492081","title":"GitHub","valid_for_s":18},"ok":true}
otpkeep code "example bank" --next  # counter-based (HOTP) account: makes and prints the next code
```

### Which account `<query>` names

The query is searched like the search panel searches: names, services, accounts, tags and notes, including
pinyin for Chinese names. Several words are joined with spaces (`otpkeep code demo git` searches `demo git`).

- One match: its code is printed.
- Several matches, and exactly one of them is **named** exactly the query (ignoring case): that one.
- Otherwise the command fails with `MULTIPLE_MATCHES` (exit code 3) and lists the candidates
  (`name · service · account`) on standard error, or in `error.candidates` with `--json`. Use the full name, or
  rename one account in the app, to be exact.
- No match: `NO_MATCH` (exit code 2).

Counter-based (HOTP) accounts move their counter each time a code is made, so they need `--next`; without it the
command fails with `INVALID_INPUT` (exit code 10) and changes nothing.

### Output

Without `--json` the code alone is printed on standard output, followed by a line break; messages go to standard
error.

With `--json` one line of JSON is printed on standard output, also for errors:

```json
{"ok":true,"data":{"code":"492081","title":"GitHub","valid_for_s":18}}
{"ok":false,"error":{"code":"MULTIPLE_MATCHES","message":"'demo' matches 2 entries; be more specific","candidates":["Demo Git · Demo Git · demo-user","Demo Git Staging · Demo Git · demo-user"]}}
```

| Field | Meaning |
|---|---|
| `data.code` | The code, digits (or five characters for Steam) without spaces |
| `data.title` | The account's name in OTPKeep |
| `data.valid_for_s` | Seconds the code stays valid; `null` for counter-based accounts |
| `error.code` | A stable error code, below |
| `error.message` | What went wrong, in English, for people |
| `error.candidates` | With `MULTIPLE_MATCHES`: the accounts that matched |

### Exit codes and error codes

| Exit | `error.code` | Meaning | What to do |
|---|---|---|---|
| 0 | — | The code was printed | |
| 2 | `NO_MATCH` | Nothing matches the query | Check the name with `otpkeep list` |
| 3 | `MULTIPLE_MATCHES` | Several accounts match | Use the full name |
| 5 | `AUTH_FAILED` | Wrong master password, or none could be read | Run in a terminal or pass `--password-stdin` |
| 6 | `BUSY` | Another OTPKeep holds the vault and could not be asked (Windows, or an older app) | Quit the other OTPKeep, or try again |
| 7 | `VAULT_ERROR` | The vault cannot be opened, or the app could not give a code | Open the app; it explains |
| 8 | `READ_ONLY` | A newer OTPKeep uses the data (only `--next` writes) | Update OTPKeep |
| 10 | `INVALID_INPUT` | Counter-based account without `--next`, or no code can be computed | Add `--next` |
| 12 | `DISABLED` | The running app does not allow scripts | Turn on **Allow scripts to get codes from the running app** in Settings |
| 64 | — | The command line was not understood | See `otpkeep help` |

## Waiting for a fresh code

A code shown with only a few seconds left may expire before the website checks it. Wait for the next one when
`valid_for_s` is small:

```sh
#!/bin/sh
# Prints a code that stays valid for at least 5 seconds.
set -eu
reply=$(otpkeep code github --json)
left=$(printf '%s' "$reply" | jq -r '.data.valid_for_s')
if [ "$left" != "null" ] && [ "$left" -lt 5 ]; then
  sleep "$((left + 1))"
  reply=$(otpkeep code github --json)
fi
printf '%s\n' "$reply" | jq -r '.data.code'
```

## Examples

### bash / zsh

```sh
code=$(otpkeep code github) || exit $?
printf 'Your code: %s\n' "$code"

# With jq, failing on any error:
otpkeep code github --json | jq -er '.data.code'
```

### fish

```fish
set code (otpkeep code github); or return
echo "Your code: $code"
```

### Python

```python
import json
import subprocess


def otp(query: str) -> str:
    """The current code of the one account matching `query`."""
    result = subprocess.run(
        ["otpkeep", "code", query, "--json"],
        capture_output=True, text=True, timeout=30,
    )
    reply = json.loads(result.stdout)
    if not reply["ok"]:
        raise RuntimeError(f'{reply["error"]["code"]}: {reply["error"]["message"]}')
    return reply["data"]["code"]


print(otp("github"))
```

### Node.js

```js
const { execFileSync } = require("node:child_process");

function otp(query) {
  let out;
  try {
    out = execFileSync("otpkeep", ["code", query, "--json"], { encoding: "utf8" });
  } catch (failed) {
    out = failed.stdout; // a non-zero exit still prints the JSON error
  }
  const reply = JSON.parse(out);
  if (!reply.ok) throw new Error(`${reply.error.code}: ${reply.error.message}`);
  return reply.data.code;
}

console.log(otp("github"));
```

Pass the query as its own argument (as above), never through a shell string built from untrusted text.

### Copying to the clipboard yourself

```sh
otpkeep code github | tr -d '\n' | pbcopy   # macOS; on Linux: wl-copy or xclip -selection clipboard
```

Note that, unlike the app, this does not keep the code out of clipboard history apps or clear it later.

### Logging in from the command line

Prefer typing the code yourself, or the app's **Search…** with `⇧Enter`, which types the code into the field
you are in. For a tool that reads a one-time code from standard input, pipe it in:

```sh
otpkeep code "example vpn" | some-vpn-client --otp-stdin
```

Avoid putting codes in command lines (`--otp 123456`): other users of the machine can see command lines.

### macOS Shortcuts, Raycast and Alfred

- **Shortcuts:** add a **Run Shell Script** action with `/usr/local/bin/otpkeep code github` (or the path
  `command -v otpkeep` prints), then **Copy to Clipboard** or **Show Result**.
- **Raycast:** a script command, for example `otp-github.sh`:

  ```sh
  #!/bin/sh
  # @raycast.schemaVersion 1
  # @raycast.title GitHub code
  # @raycast.mode silent
  otpkeep code github | tr -d '\n' | pbcopy && echo "Copied"
  ```

- **Alfred:** a workflow with a **Run Script** action (`/bin/sh`, `otpkeep code "{query}"`) connected to
  **Copy to Clipboard**.

Launchers start scripts with a short `PATH`; use the full path to `otpkeep` if it is not found.

### CI and shared machines

Do not use OTPKeep on shared machines or CI runners: anyone who can run programs as the same user can get your
codes. Keep CI two-factor secrets in the CI system's own secret store.

## The socket protocol

The command line is the supported, stable interface. For tools that cannot run a command, the app's socket is
documented here too; its messages carry a version (`"v":1`), and a change that is not backwards compatible gets a
new version.

| | |
|---|---|
| macOS | `~/Library/Application Support/OTPKeep/app.sock` |
| Linux | `$XDG_DATA_HOME/otpkeep/app.sock` (default `~/.local/share/otpkeep/app.sock`) |
| Windows | none yet |
| `OTPKEEP_DATA_DIR` set | `$OTPKEEP_DATA_DIR/app.sock` |

- A Unix domain stream socket. Its folder is `0700` and the socket `0600`: only the user running the app can
  connect.
- **One request per connection:** send one line of JSON ending in a line break, read one line back; the app then
  closes the connection.
- Each line is at most **4096 bytes**. Each side waits at most **2 seconds** to send or receive.
- The app answers only while running; the socket is removed when it quits.

Request (`next` may be left out, and is `false` then):

```json
{"v":1,"op":"code","query":"github","next":false}
```

Answers:

```json
{"v":1,"ok":true,"name":"GitHub","code":"492081","valid_for_s":18}
{"v":1,"ok":true,"name":"Example Bank","code":"755224"}
{"v":1,"ok":false,"error":"AMBIGUOUS","message":"'demo' matches 2 accounts; be more specific","candidates":["Demo Git · Demo Git · demo-user","Demo Git Staging · Demo Git · demo-user"]}
{"v":1,"ok":false,"error":"DISABLED","message":"scripts may not get codes from the running app; turn on \"Allow scripts to get codes\" in OTPKeep's Settings"}
```

`error` is one of `LOCKED` (the app waits for its master password), `NOT_FOUND`, `AMBIGUOUS`, `NEEDS_NEXT`
(a counter-based account asked without `"next":true`), `DISABLED`, `UNAVAILABLE` (the app cannot open its data),
`READ_ONLY`, `BAD_REQUEST` and `FAILED`.

With `nc` (BSD netcat, as on macOS):

```sh
printf '%s\n' '{"v":1,"op":"code","query":"github"}' | nc -U ~/Library/Application\ Support/OTPKeep/app.sock
```

With Python:

```python
import json
import os
import socket

path = os.path.expanduser("~/Library/Application Support/OTPKeep/app.sock")
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
    s.settimeout(2)
    s.connect(path)
    s.sendall(json.dumps({"v": 1, "op": "code", "query": "github"}).encode() + b"\n")
    reply = json.loads(s.makefile().readline())
print(reply["code"] if reply["ok"] else reply["error"])
```

## Security

- **Who can ask:** only programs running as your user on this computer. The socket is not on the network, and its
  folder and file are readable only by you.
- **What it never does:** it never gives secrets, only current codes; codes are never written to logs; it does
  not listen while the app is closed; codes given to scripts are not counted as recently used.
- **With a master password**, scripts are off until you turn them on, because once the app is unlocked they would
  get codes without the password.
- **To turn it off:** Settings → **Allow scripts to get codes from the running app**. The command line then
  answers `DISABLED` while the app runs, and still works when the app is closed (asking for the master password
  if there is one).

## Troubleshooting

| Problem | Cause and fix |
|---|---|
| `DISABLED` | Turn on **Allow scripts to get codes from the running app** in Settings. |
| `BUSY` after 5 seconds | On Windows, or with an OTPKeep older than this feature: quit the app and try again. |
| `AUTH_FAILED` in a script | The app is locked or closed and the vault has a master password: unlock the app (and allow scripts), or pass the password with `--password-stdin`. |
| `MULTIPLE_MATCHES` | Use the account's full name; `otpkeep list` shows the names. |
| `otpkeep: command not found` from a launcher | Use the full path (`command -v otpkeep` in a terminal shows it). |
| `nc` prints nothing | Use BSD netcat (`nc -U`); some netcat versions need `-q 1` or do not support `-U`. Check that the app is running and the socket exists. |
