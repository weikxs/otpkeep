// Regenerates crates/keep-transfer/src/goldenpassport/fixtures/*.secrets (demo data only):
//   swiftc -o /tmp/gp-fixtures scripts/goldenpassport-fixtures.swift && /tmp/gp-fixtures crates/keep-transfer/src/goldenpassport/fixtures
// Writes .secrets files exactly the way GoldenPassport 0.1.x does:
// NSKeyedArchiver.archivedData(withRootObject: [String: String]).
import Foundation

let dir = CommandLine.arguments[1]
func write(_ name: String, _ data: [String: String]) {
    let bytes = NSKeyedArchiver.archivedData(withRootObject: data)
    try! bytes.write(to: URL(fileURLWithPath: dir + "/" + name))
}
write("demo-0.1.secrets", [
    "Example Cloud": "otpauth://totp/Example%20Cloud:alex@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example%20Cloud",
    "示例银行 个人": "otpauth://totp/%E7%A4%BA%E4%BE%8B%E9%93%B6%E8%A1%8C:alex?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=%E7%A4%BA%E4%BE%8B%E9%93%B6%E8%A1%8C",
    "Demo Git": "otpauth://totp/Demo Git:demo-user?secret=GEZDGNBVGY3TQOJQ&algorithm=SHA256&digits=8&period=60",
    "Broken": "otpauth://totp/Broken?issuer=NoSecret",
])
write("empty-0.1.secrets", [:])
