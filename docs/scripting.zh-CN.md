# 在脚本中使用 OTPKeep

[English](scripting.md)

`otpkeep code` 打印某个账号的当前验证码，供 shell 脚本、Python、Node.js、启动器和自动化工具填写两步验证码。本页是这个接口的完整说明。

## 什么时候可用

| 情况 | `otpkeep code` 的做法 |
|---|---|
| OTPKeep App **没有**运行 | 自己打开数据。设置了主口令时，从终端或 `--password-stdin` 读取主口令。 |
| App 正在运行且已解锁（macOS、Linux） | 通过一个私有的本地套接字向 App 询问，立即打印结果。 |
| App 正在运行但还在等主口令 | 自己打开数据（App 未解锁时不占用数据）。 |
| App 正在运行且**不允许脚本** | 立即以 `DISABLED` 失败（退出码 12）。 |
| App 在 Windows 上运行 | 暂不支持：命令最多等待 5 秒，然后以 `BUSY` 失败（退出码 6）。请先退出 App，或直接用 App。 |

只有打开 **设置 → 允许脚本从运行中的 OTPKeep 获取验证码** 时，App 才会回答：

- 数据的密钥保存在系统钥匙串中（没有主口令）时**默认开启**。这种情况下命令行本来就能直接打开数据，开启它不会增加新的访问途径。
- 数据设置了主口令时**默认关闭**。开启后，App 解锁期间你运行的任何程序都能不输口令取得验证码。只有确实需要时才开启。

脚本取得的验证码**不**算作"最近使用"：菜单栏菜单仍然只显示你自己用过的账号。

## 快速上手

```sh
otpkeep code github                 # 打印 492081
otpkeep code github --json          # 打印 {"data":{"code":"492081","title":"GitHub","valid_for_s":18},"ok":true}
otpkeep code "example bank" --next  # 计数器型（HOTP）账号：生成并打印下一个验证码
```

### `<query>` 指的是哪个账号

查询方式与搜索面板相同：名称、服务商、账号、标签和备注都会被搜索，中文名称也支持拼音。多个词以空格连接（`otpkeep code demo git` 搜索 `demo git`）。

- 只有一个匹配：打印它的验证码。
- 有多个匹配，且其中恰好一个的**名称**与查询完全相同（不区分大小写）：用这个。
- 否则以 `MULTIPLE_MATCHES` 失败（退出码 3），并在标准错误中列出候选（`名称 · 服务商 · 账号`），加 `--json` 时在 `error.candidates` 中给出。请使用完整名称，或在 App 中改名，使查询唯一。
- 没有匹配：`NO_MATCH`（退出码 2）。

计数器型（HOTP）账号每生成一次验证码计数器就前进一步，因此需要 `--next`；不加时命令以 `INVALID_INPUT`（退出码 10）失败，不做任何改动。

### 输出

不加 `--json` 时，标准输出只打印验证码和一个换行；提示信息写到标准错误。

加 `--json` 时，标准输出打印一行 JSON，出错时也是：

```json
{"ok":true,"data":{"code":"492081","title":"GitHub","valid_for_s":18}}
{"ok":false,"error":{"code":"MULTIPLE_MATCHES","message":"'demo' matches 2 entries; be more specific","candidates":["Demo Git · Demo Git · demo-user","Demo Git Staging · Demo Git · demo-user"]}}
```

| 字段 | 含义 |
|---|---|
| `data.code` | 验证码，不含空格（Steam 为 5 个字符） |
| `data.title` | 账号在 OTPKeep 中的名称 |
| `data.valid_for_s` | 验证码还剩多少秒有效；计数器型账号为 `null` |
| `error.code` | 稳定的错误代码，见下表 |
| `error.message` | 给人看的英文说明 |
| `error.candidates` | `MULTIPLE_MATCHES` 时匹配到的账号 |

### 退出码与错误代码

| 退出码 | `error.code` | 含义 | 怎么办 |
|---|---|---|---|
| 0 | — | 已打印验证码 | |
| 2 | `NO_MATCH` | 没有匹配的账号 | 用 `otpkeep list` 查看名称 |
| 3 | `MULTIPLE_MATCHES` | 匹配到多个账号 | 使用完整名称 |
| 5 | `AUTH_FAILED` | 主口令错误，或无法读取主口令 | 在终端中运行，或使用 `--password-stdin` |
| 6 | `BUSY` | 另一个 OTPKeep 占用数据且无法询问（Windows，或旧版 App） | 退出另一个 OTPKeep 后重试 |
| 7 | `VAULT_ERROR` | 数据无法打开，或 App 给不出验证码 | 打开 App，它会说明原因 |
| 8 | `READ_ONLY` | 新版 OTPKeep 正在使用这些数据（只有 `--next` 会写入） | 更新 OTPKeep |
| 10 | `INVALID_INPUT` | 计数器型账号没加 `--next`，或无法计算验证码 | 加上 `--next` |
| 12 | `DISABLED` | 运行中的 App 不允许脚本 | 在设置中打开 **允许脚本从运行中的 OTPKeep 获取验证码** |
| 64 | — | 无法理解命令行 | 参见 `otpkeep help` |

## 等待新的验证码

剩余时间很短的验证码可能在网站校验前过期。`valid_for_s` 很小时，等下一个：

```sh
#!/bin/sh
# 打印一个至少还能用 5 秒的验证码。
set -eu
reply=$(otpkeep code github --json)
left=$(printf '%s' "$reply" | jq -r '.data.valid_for_s')
if [ "$left" != "null" ] && [ "$left" -lt 5 ]; then
  sleep "$((left + 1))"
  reply=$(otpkeep code github --json)
fi
printf '%s\n' "$reply" | jq -r '.data.code'
```

## 示例

### bash / zsh

```sh
code=$(otpkeep code github) || exit $?
printf '验证码：%s\n' "$code"

# 使用 jq，出错即失败：
otpkeep code github --json | jq -er '.data.code'
```

### fish

```fish
set code (otpkeep code github); or return
echo "验证码：$code"
```

### Python

```python
import json
import subprocess


def otp(query: str) -> str:
    """与 query 匹配的那个账号的当前验证码。"""
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
    out = failed.stdout; // 退出码非 0 时仍会打印 JSON 错误
  }
  const reply = JSON.parse(out);
  if (!reply.ok) throw new Error(`${reply.error.code}: ${reply.error.message}`);
  return reply.data.code;
}

console.log(otp("github"));
```

把查询作为单独的参数传入（如上），不要用不可信的文本拼接 shell 命令。

### 自己复制到剪贴板

```sh
otpkeep code github | tr -d '\n' | pbcopy   # macOS；Linux 上用 wl-copy 或 xclip -selection clipboard
```

注意：与 App 不同，这样复制的验证码不会避开剪贴板历史工具，也不会稍后自动清除。

### 在命令行中登录

最好自己输入验证码，或使用 App 的 **搜索…** 并按 `⇧回车`，把验证码直接输入到当前的输入框。对于从标准输入读取一次性密码的工具，用管道传入：

```sh
otpkeep code "example vpn" | some-vpn-client --otp-stdin
```

不要把验证码放在命令行参数里（`--otp 123456`）：本机其他用户能看到命令行。

### macOS 快捷指令、Raycast 和 Alfred

- **快捷指令：** 添加 **运行 Shell 脚本** 操作，内容为 `/usr/local/bin/otpkeep code github`（或在终端里 `command -v otpkeep` 显示的路径），再接 **拷贝至剪贴板** 或 **显示结果**。
- **Raycast：** 写一个脚本命令，例如 `otp-github.sh`：

  ```sh
  #!/bin/sh
  # @raycast.schemaVersion 1
  # @raycast.title GitHub code
  # @raycast.mode silent
  otpkeep code github | tr -d '\n' | pbcopy && echo "Copied"
  ```

- **Alfred：** 在工作流中添加 **Run Script** 操作（`/bin/sh`，`otpkeep code "{query}"`），连到 **Copy to Clipboard**。

启动器运行脚本时 `PATH` 很短；找不到 `otpkeep` 时请写完整路径。

### CI 与共用电脑

不要在共用电脑或 CI 机器上使用 OTPKeep：能以同一用户身份运行程序的人都能取得你的验证码。CI 用的两步验证密钥请放在 CI 系统自己的密钥库里。

## 套接字协议

命令行是受支持、稳定的接口。对于不能运行命令的工具，这里也说明 App 的套接字；消息带有版本号（`"v":1`），不向后兼容的改动会使用新的版本号。

| | |
|---|---|
| macOS | `~/Library/Application Support/OTPKeep/app.sock` |
| Linux | `$XDG_DATA_HOME/otpkeep/app.sock`（默认 `~/.local/share/otpkeep/app.sock`） |
| Windows | 暂无 |
| 设置了 `OTPKEEP_DATA_DIR` | `$OTPKEEP_DATA_DIR/app.sock` |

- Unix 域流式套接字。所在目录权限为 `0700`，套接字为 `0600`：只有运行 App 的用户能连接。
- **每个连接一个请求：** 发送一行以换行结尾的 JSON，读回一行；之后 App 关闭连接。
- 每行最多 **4096 字节**。双方收发各最多等待 **2 秒**。
- 只有 App 运行时才回答；App 退出时删除套接字。

请求（`next` 可省略，省略即 `false`）：

```json
{"v":1,"op":"code","query":"github","next":false}
```

回答：

```json
{"v":1,"ok":true,"name":"GitHub","code":"492081","valid_for_s":18}
{"v":1,"ok":true,"name":"Example Bank","code":"755224"}
{"v":1,"ok":false,"error":"AMBIGUOUS","message":"'demo' matches 2 accounts; be more specific","candidates":["Demo Git · Demo Git · demo-user","Demo Git Staging · Demo Git · demo-user"]}
{"v":1,"ok":false,"error":"DISABLED","message":"scripts may not get codes from the running app; turn on \"Allow scripts to get codes\" in OTPKeep's Settings"}
```

`error` 取值为 `LOCKED`（App 在等主口令）、`NOT_FOUND`、`AMBIGUOUS`、`NEEDS_NEXT`（计数器型账号的请求没有 `"next":true`）、`DISABLED`、`UNAVAILABLE`（App 无法打开数据）、`READ_ONLY`、`BAD_REQUEST` 或 `FAILED`。

使用 `nc`（BSD netcat，macOS 自带）：

```sh
printf '%s\n' '{"v":1,"op":"code","query":"github"}' | nc -U ~/Library/Application\ Support/OTPKeep/app.sock
```

使用 Python：

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

## 安全

- **谁能询问：** 只有这台电脑上以你的身份运行的程序。套接字不在网络上，其目录和文件只有你能访问。
- **它从不做的事：** 只给当前验证码，从不给密钥；验证码不写入日志；App 关闭时不监听；脚本取得的验证码不算作最近使用。
- **设置了主口令时**，脚本默认关闭，因为 App 解锁后脚本可以不输口令取得验证码。
- **关闭方法：** 设置 → **允许脚本从运行中的 OTPKeep 获取验证码**。关闭后 App 运行时命令行回答 `DISABLED`；App 关闭时命令行照常工作（有主口令时会询问）。

## 故障排查

| 问题 | 原因与解决 |
|---|---|
| `DISABLED` | 在设置中打开 **允许脚本从运行中的 OTPKeep 获取验证码**。 |
| 5 秒后 `BUSY` | 在 Windows 上，或 OTPKeep 版本早于此功能：退出 App 后重试。 |
| 脚本中出现 `AUTH_FAILED` | App 已锁定或已关闭，且数据有主口令：解锁 App（并允许脚本），或用 `--password-stdin` 传入口令。 |
| `MULTIPLE_MATCHES` | 使用账号的完整名称；`otpkeep list` 会列出名称。 |
| 启动器报 `otpkeep: command not found` | 使用完整路径（在终端运行 `command -v otpkeep` 查看）。 |
| `nc` 没有输出 | 使用 BSD netcat（`nc -U`）；有些 netcat 需要 `-q 1` 或不支持 `-U`。确认 App 正在运行且套接字存在。 |
