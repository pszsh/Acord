import SwiftUI
import UIKit

/// SwiftUI wrapper around the UIView that hosts the Rust viewport.
struct IcedViewportRepresentable: UIViewRepresentable {
    func makeUIView(context: Context) -> IcedViewportView {
        IcedViewportView(frame: .zero)
    }

    func updateUIView(_ uiView: IcedViewportView, context: Context) {
        // size pushed via setFrameSize; nothing to refresh per SwiftUI tick.
    }
}
