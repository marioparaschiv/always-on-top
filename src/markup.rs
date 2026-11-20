//! XAML markup for the settings page, its list rows and keycaps.

pub const XMLNS: &str = r#"xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml""#;
const CARD: &str = r#"Background="{ThemeResource CardBackgroundFillColorDefaultBrush}" BorderBrush="{ThemeResource CardStrokeColorDefaultBrush}" BorderThickness="1" CornerRadius="{ThemeResource ControlCornerRadius}""#;
pub const SECONDARY: &str = r#"Foreground="{ThemeResource TextFillColorSecondaryBrush}""#;

/// Nucleo Micro Bold "pin-tack-filled" (20px grid), as XAML path data.
const PIN_TACK: &str = "M 10 13 h 6 c -0.0526 -0.5187 -0.2012 -1.346 -0.6667 -2.25 -0.4304 -0.8359 -0.962 -1.4079 -1.3333 -1.75 v -3 c 0 -1.6569 -1.3431 -3 -3 -3 h -1 s -1 0 -1 0 c -1.6569 0 -3 1.3431 -3 3 v 3 c -0.3713 0.3421 -0.903 0.9141 -1.3333 1.75 -0.4655 0.904 -0.6141 1.7313 -0.6667 2.25 h 6 Z";

/// The app icon (same as `assets/icon.svg`) drawn in XAML at `size` px.
fn app_icon(size: u32) -> String {
    format!(
        r##"<Grid Width="{size}" Height="{size}" VerticalAlignment="Center">
            <Border CornerRadius="{radius}">
                <Border.Background>
                    <LinearGradientBrush StartPoint="0,0" EndPoint="1,1">
                        <GradientStop Color="#4AA0F0" Offset="0"/><GradientStop Color="#0F5FC0" Offset="1"/>
                    </LinearGradientBrush>
                </Border.Background>
            </Border>
            <Viewbox Width="{glyph}" Height="{glyph}">
                <Canvas Width="20" Height="20" RenderTransformOrigin="0.5,0.5">
                    <Canvas.RenderTransform><RotateTransform Angle="40"/></Canvas.RenderTransform>
                    <Path Data="M 10 17 L 10 13" Stroke="White" StrokeThickness="2" StrokeStartLineCap="Round" StrokeEndLineCap="Round"/>
                    <Path Data="{PIN_TACK}" Fill="White" Stroke="White" StrokeThickness="2" StrokeLineJoin="Round"/>
                </Canvas>
            </Viewbox>
        </Grid>"##,
        radius = size as f32 * 0.225,
        glyph = size as f32 * 0.62,
    )
}

/// A settings card: label and description on the left, `control` on the right.
fn card(label: &str, description: &str, control: &str) -> String {
    format!(
        r#"<Border {CARD} Padding="16,12" MinHeight="68">
            <Grid ColumnSpacing="16" VerticalAlignment="Center">
                <Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
                <StackPanel VerticalAlignment="Center">
                    <TextBlock Text="{label}"/>
                    <TextBlock Text="{description}" Style="{{StaticResource CaptionTextBlockStyle}}" {SECONDARY} TextWrapping="Wrap"/>
                </StackPanel>
                <Border Grid.Column="1" VerticalAlignment="Center">{control}</Border>
            </Grid>
        </Border>"#
    )
}

/// Escapes text for XML (XAML here, the scheduled task in `autostart.rs`).
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn keycap(key: &str) -> String {
    let key = escape(key);
    format!(
        r#"<Border {XMLNS} Background="{{ThemeResource AccentFillColorDefaultBrush}}" CornerRadius="{{ThemeResource ControlCornerRadius}}" Padding="10,5" MinWidth="36">
            <TextBlock Text="{key}" FontWeight="SemiBold" HorizontalAlignment="Center" Foreground="{{ThemeResource TextOnAccentFillColorPrimaryBrush}}"/>
        </Border>"#
    )
}

fn toggle(name: &str) -> String {
    format!(r#"<ToggleSwitch x:Name="{name}" OnContent="" OffContent="" MinWidth="0"/>"#)
}

pub fn page() -> String {
    // The keycaps are a button that opens the shortcut recorder, as in PowerToys.
    let shortcut = r#"<Button x:Name="EditShortcut" Padding="6" ToolTipService.ToolTip="Change shortcut">
            <StackPanel Orientation="Horizontal" Spacing="8">
                <StackPanel x:Name="ShortcutKeys" Orientation="Horizontal" Spacing="4"/>
                <FontIcon Glyph="&#xE70F;" FontSize="14" Margin="0,0,4,0"/>
            </StackPanel>
        </Button>"#;
    format!(
        r#"<Grid {XMLNS}>
            <Grid.RowDefinitions><RowDefinition Height="32"/><RowDefinition Height="*"/></Grid.RowDefinitions>
            <StackPanel Orientation="Horizontal" Spacing="10" Margin="14,0,0,0" VerticalAlignment="Center">
                {}
                <TextBlock Text="Always on Top" Style="{{StaticResource CaptionTextBlockStyle}}" VerticalAlignment="Center"/>
            </StackPanel>
            <ScrollViewer Grid.Row="1">
                <StackPanel Padding="24,8,24,24" Spacing="4">
                    <TextBlock Text="Always on Top" Style="{{StaticResource TitleTextBlockStyle}}"/>
                    <TextBlock Text="Pin any window above all others." {SECONDARY} Margin="0,0,0,16"/>
                    <InfoBar x:Name="HotkeyError" Severity="Error" IsClosable="False" Margin="0,0,0,8"
                        Title="Shortcut unavailable"/>
                    {}
                    {}
                    {}
                    {}
                    {}
                    {}
                    <Grid Margin="0,24,0,4">
                        <TextBlock x:Name="PinnedHeader" Style="{{StaticResource BodyStrongTextBlockStyle}}" VerticalAlignment="Center"/>
                        <Button x:Name="UnpinAll" Content="Unpin all" HorizontalAlignment="Right"/>
                    </Grid>
                    <TextBlock x:Name="EmptyText" {SECONDARY} TextWrapping="Wrap"/>
                    <StackPanel x:Name="PinnedList" Spacing="4"/>
                </StackPanel>
            </ScrollViewer>
        </Grid>"#,
        app_icon(16),
        card("Activation shortcut", "Pin or unpin the active window", shortcut),
        card("Show a colored border", "Accent outline around pinned windows (Windows 11)", &toggle("BorderToggle")),
        card("Play a sound", "When a window is pinned or unpinned", &toggle("SoundToggle")),
        card("Show tray icon", "When hidden, start Always on Top again to open this window", &toggle("TrayIconToggle")),
        card("Start with Windows", "Run in the tray when you sign in", &toggle("AutostartToggle")),
        card("Run as administrator", "Needed to pin apps that run as administrator. Asks for permission.", &toggle("AdminToggle")),
    )
}

pub fn row() -> String {
    format!(
        r#"<Border {XMLNS} {CARD} Padding="16,8" MinHeight="52">
            <Grid ColumnSpacing="12" VerticalAlignment="Center">
                <Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
                <TextBlock x:Name="Title" TextTrimming="CharacterEllipsis" VerticalAlignment="Center"/>
                <Button x:Name="Unpin" Grid.Column="1" Content="Unpin"/>
            </Grid>
        </Border>"#
    )
}
