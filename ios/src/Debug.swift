import Foundation

/// Gated logging — every diagnostic print in the iOS shell goes through here.
/// Release builds compile this to a no-op so no log lines leak into shipping.
/// Define DEBUG via `-D DEBUG` when invoking swiftc (debug.sh does this; the
/// release path used by install.sh does not).
@inline(__always)
func dlog(_ message: @autoclosure () -> String, file: StaticString = #file, line: UInt = #line) {
    #if DEBUG
    let stem = (("\(file)" as NSString).lastPathComponent as NSString).deletingPathExtension
    print("[Acord] \(stem):\(line) — \(message())")
    #endif
}
