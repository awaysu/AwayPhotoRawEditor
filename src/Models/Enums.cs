namespace AwayPhotoRawEditor.Models;

/// <summary>Discrete image rotation (degrees clockwise).</summary>
public enum Rotation
{
    R0 = 0,
    R90 = 90,
    R180 = 180,
    R270 = 270
}

/// <summary>Watermark anchor corner.</summary>
public enum WatermarkPosition
{
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight
}

/// <summary>Watermark text colour (order matches the export dialog dropdown).</summary>
public enum WatermarkColor
{
    White,
    Black,
    Blue,
    Yellow,
    Green,
    Red,
    Gray,
    Orange
}

/// <summary>Active editing tool in the Tools panel / viewer overlay.</summary>
public enum ToolMode
{
    None,
    Crop,
    Gradient,
    Heal
}

/// <summary>Healing brush behaviour.</summary>
public enum HealMode
{
    Clone,
    Inpaint
}

/// <summary>Viewer zoom presets.</summary>
public enum ZoomMode
{
    Fit,
    Actual100,
    Actual200,
    Custom
}
