// Reads a .secrets file the way GoldenPassport 0.1.x imports it, and parses every link the way its
// OTPAuthURLParser does, but reports failures instead of crashing on them.
// Usage: swift scripts/goldenpassport-check.swift <file>
// Prints "ok <count>" and exits 0, or prints the problem and exits 1.
import Foundation

let path = CommandLine.arguments[1]
guard let object = NSKeyedUnarchiver.unarchiveObject(withFile: path) else {
    print("not a keyed archive"); exit(1)
}
guard let data = object as? [String: String] else {
    print("not a [String: String] dictionary"); exit(1)
}
for (name, link) in data {
    // OTPAuthURLParser: URL(string:), url.host!, url.query!, the "secret=" component split on "=".
    guard let url = URL(string: link), url.host != nil, let query = url.query else {
        print("unparsable link for \(name)"); exit(1)
    }
    let parts = query.components(separatedBy: "&").filter { $0.hasPrefix("secret=") }
    guard let secret = parts.first?.components(separatedBy: "="), secret.count == 2, !secret[1].isEmpty else {
        print("no usable secret for \(name)"); exit(1)
    }
}
print("ok \(data.count)")
