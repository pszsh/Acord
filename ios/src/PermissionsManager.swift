import UIKit
import Photos
import AVFoundation

/// Drives the iOS permission dialogs that fire on first launch.
/// Photos / Camera / Microphone trigger native system prompts when their
/// `requestAuthorization` is called, the Info.plist usage strings exist,
/// and the corresponding framework is linked.
/// File-system access on iOS isn't a global permission — DocumentPicker's
/// per-file picker IS the consent moment for that, and asCopy:true means
/// no entitlements beyond the picker itself are required.
enum PermissionsManager {
    /// Sequentially asks for Photos → Camera → Microphone access.
    /// Each call shows the system prompt only when status is .notDetermined.
    static func requestSystemPermissions() {
        dlog("requestSystemPermissions called")
        let photosStatus = PHPhotoLibrary.authorizationStatus(for: .readWrite)
        dlog("photos status: \(authStatusName(photosStatus))")
        if photosStatus == .notDetermined {
            dlog("photos: requesting authorization (system dialog should appear)")
            PHPhotoLibrary.requestAuthorization(for: .readWrite) { status in
                dlog("photos: dialog returned \(authStatusName(status))")
                requestCamera()
            }
        } else {
            dlog("photos: already determined — skipping dialog")
            requestCamera()
        }
    }

    private static func requestCamera() {
        let cameraStatus = AVCaptureDevice.authorizationStatus(for: .video)
        dlog("camera status: \(avStatusName(cameraStatus))")
        if cameraStatus == .notDetermined {
            dlog("camera: requesting authorization (system dialog should appear)")
            AVCaptureDevice.requestAccess(for: .video) { granted in
                dlog("camera: dialog returned granted=\(granted)")
                requestMicrophone()
            }
        } else {
            dlog("camera: already determined — skipping dialog")
            requestMicrophone()
        }
    }

    private static func requestMicrophone() {
        let micStatus = AVCaptureDevice.authorizationStatus(for: .audio)
        dlog("microphone status: \(avStatusName(micStatus))")
        if micStatus == .notDetermined {
            dlog("microphone: requesting authorization (system dialog should appear)")
            AVCaptureDevice.requestAccess(for: .audio) { granted in
                dlog("microphone: dialog returned granted=\(granted)")
            }
        } else {
            dlog("microphone: already determined — skipping dialog")
        }
    }

    private static func authStatusName(_ s: PHAuthorizationStatus) -> String {
        switch s {
        case .notDetermined: return "notDetermined"
        case .restricted:    return "restricted"
        case .denied:        return "denied"
        case .authorized:    return "authorized"
        case .limited:       return "limited"
        @unknown default:    return "unknown(\(s.rawValue))"
        }
    }

    private static func avStatusName(_ s: AVAuthorizationStatus) -> String {
        switch s {
        case .notDetermined: return "notDetermined"
        case .restricted:    return "restricted"
        case .denied:        return "denied"
        case .authorized:    return "authorized"
        @unknown default:    return "unknown(\(s.rawValue))"
        }
    }
}
