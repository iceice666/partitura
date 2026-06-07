import SwiftUI

enum Motion {
    static func standard(reduceMotion: Bool) -> Animation? {
        reduceMotion ? .easeOut(duration: 0.12) : .spring(response: 0.28, dampingFraction: 0.82)
    }

    static func toast(reduceMotion: Bool) -> Animation? {
        reduceMotion ? .easeOut(duration: 0.12) : .snappy(duration: 0.22)
    }

    static func pulse(reduceMotion: Bool) -> Animation? {
        reduceMotion ? nil : .easeInOut(duration: 0.9).repeatForever(autoreverses: true)
    }

    static func shimmer(reduceMotion: Bool) -> Animation? {
        reduceMotion ? nil : .linear(duration: 1.2).repeatForever(autoreverses: false)
    }
}
