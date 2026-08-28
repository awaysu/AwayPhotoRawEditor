import Foundation

/// Discrete image rotation (degrees clockwise).
public enum Rotation: Int, Codable, Sendable, CaseIterable {
    case r0 = 0, r90 = 90, r180 = 180, r270 = 270

    /// The name .NET's XmlSerializer writes for this value.
    public var xmlName: String {
        switch self {
        case .r0: return "R0"
        case .r90: return "R90"
        case .r180: return "R180"
        case .r270: return "R270"
        }
    }

    public init?(xmlName: String) {
        switch xmlName {
        case "R0": self = .r0
        case "R90": self = .r90
        case "R180": self = .r180
        case "R270": self = .r270
        default: return nil
        }
    }

    /// Turning clockwise by one 90° step.
    public var next: Rotation {
        switch self {
        case .r0: return .r90
        case .r90: return .r180
        case .r180: return .r270
        case .r270: return .r0
        }
    }

    public var previous: Rotation {
        switch self {
        case .r0: return .r270
        case .r90: return .r0
        case .r180: return .r90
        case .r270: return .r180
        }
    }
}

/// Watermark anchor corner.
public enum WatermarkPosition: Int, Codable, Sendable, CaseIterable {
    case topLeft, topRight, bottomLeft, bottomRight

    public var xmlName: String {
        switch self {
        case .topLeft: return "TopLeft"
        case .topRight: return "TopRight"
        case .bottomLeft: return "BottomLeft"
        case .bottomRight: return "BottomRight"
        }
    }

    public init?(xmlName: String) {
        guard let v = Self.allCases.first(where: { $0.xmlName == xmlName }) else { return nil }
        self = v
    }
}

/// Watermark text colour (order matches the export dialog dropdown).
public enum WatermarkColor: Int, Codable, Sendable, CaseIterable {
    case white, black, blue, yellow, green, red, gray, orange

    public var xmlName: String {
        switch self {
        case .white: return "White"
        case .black: return "Black"
        case .blue: return "Blue"
        case .yellow: return "Yellow"
        case .green: return "Green"
        case .red: return "Red"
        case .gray: return "Gray"
        case .orange: return "Orange"
        }
    }

    public init?(xmlName: String) {
        guard let v = Self.allCases.first(where: { $0.xmlName == xmlName }) else { return nil }
        self = v
    }

    /// RGB as the Windows build draws it.
    public var rgb: (r: Double, g: Double, b: Double) {
        switch self {
        case .white:  return (255, 255, 255)
        case .black:  return (0, 0, 0)
        case .blue:   return (40, 120, 240)
        case .yellow: return (245, 210, 40)
        case .green:  return (60, 190, 90)
        case .red:    return (230, 50, 50)
        case .gray:   return (150, 150, 150)
        case .orange: return (245, 150, 40)
        }
    }
}

/// Active editing tool in the Tools panel / viewer overlay.
public enum ToolMode: Int, Sendable {
    case none, crop, gradient, heal
}

/// Healing brush behaviour.
public enum HealMode: Int, Sendable {
    case clone, inpaint
}

/// Viewer zoom presets.
public enum ZoomMode: Int, Sendable {
    case fit, actual100, actual200, custom
}
