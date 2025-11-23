# Font and Icon Support in Nexus Client

This document explains how to add custom fonts and icon support to the Nexus BBS client using Iced.

## Current Unicode Icons

We're already using Unicode symbols that work in most fonts:

- `●` Connected status (U+25CF)
- `○` Disconnected status (U+25CB)
- `⚠` Warnings (U+26A0)
- `ℹ` Info messages (U+2139)
- `***` System messages

## Option 1: Extended Unicode Symbols (No Dependencies)

These symbols are widely supported in system fonts:

```rust
// Navigation
"◄►▲▼"  // Arrows
"←→↑↓"  // Arrow alternatives

// Status
"✓✗×"   // Check/cross marks
"★☆"    // Stars
"●○◉◎"  // Circles (already using)

// UI
"⚙⚡⚐"   // Settings/power/flag
"⌂⎋⏎"   // Home/escape/enter
```

## Option 2: Nerd Fonts (Best for BBS Aesthetic)

Nerd Fonts include thousands of icons perfect for terminal UIs.

### Installation Steps:

1. **Download a Nerd Font**
   - Visit: https://www.nerdfonts.com/
   - Recommended: "FiraCode Nerd Font" or "JetBrains Mono Nerd Font"
   - Download and install on your system

2. **Add Font to Project** (for bundling)
   ```
   nexus-client/
   ├── fonts/
   │   └── FiraCodeNerdFont-Regular.ttf
   ```

3. **Load Font in Iced**

   ```rust
   use iced::Font;
   
   const NERD_FONT: Font = Font::with_name("FiraCode Nerd Font");
   
   // Or embed in binary:
   const NERD_FONT_BYTES: &[u8] = include_bytes!("../fonts/FiraCodeNerdFont-Regular.ttf");
   const NERD_FONT: Font = Font::External {
       name: "Nerd Font",
       bytes: NERD_FONT_BYTES,
   };
   ```

4. **Use Icons in Text**

   ```rust
   // Nerd Font icons (examples)
   text("  Connected")           // Computer icon
   text("  Disconnected")        // Computer off
   text("  Users")               // Users icon
   text("  Chat")                // Chat bubble
   text("  Settings")            // Gear icon
   text("  Add")                 // Plus in circle
   text("  Delete")              // Trash can
   text("  Warning")             // Triangle warning
   text("  Server")              // Server icon
   text("  Network")             // Network icon
   ```

### Nerd Font Icon Reference

Common icons for BBS client:

| Icon | Codepoint | Description |
|------|-----------|-------------|
|  | U+F109 | Desktop/Computer |
|  | U+F233 | Server |
|  | U+F0C0 | Users/Group |
|  | U+F075 | Chat/Comment |
|  | U+F013 | Settings/Gear |
|  | U+F067 | Plus/Add |
|  | U+F1F8 | Trash/Delete |
|  | U+F071 | Warning Triangle |
|  | U+F0E8 | Sitemap/Network |
|  | U+F023 | Lock/Secure |
|  | U+F05E | Ban/Disconnect |
|  | U+F00C | Check/Success |
|  | U+F00D | Cross/Error |
|  | U+F0AD | Wrench/Admin |
|  | U+F304 | Edit/Pencil |

## Option 3: Material Design Icons Font

Another popular choice with clean, modern icons.

### Installation:

1. **Download Material Icons Font**
   - Visit: https://github.com/google/material-design-icons
   - Download MaterialIcons-Regular.ttf

2. **Add to Project**
   ```
   nexus-client/fonts/MaterialIcons-Regular.ttf
   ```

3. **Use in Code**
   ```rust
   const MATERIAL_ICONS: Font = Font::External {
       name: "Material Icons",
       bytes: include_bytes!("../fonts/MaterialIcons-Regular.ttf"),
   };
   
   // Material Design icons
   text("add")          // Add icon
   text("delete")       // Delete icon
   text("person")       // Person icon
   text("settings")     // Settings icon
   text("chat")         // Chat icon
   ```

## Recommended Approach

For Nexus BBS Client, we recommend:

1. **Primary**: Keep using Unicode symbols for maximum compatibility
2. **Optional**: Bundle a Nerd Font for users who want rich icons
3. **Fallback**: Always have Unicode alternatives

### Implementation Example:

```rust
// In main.rs or a new icons.rs module
pub const ICON_CONNECTED: &str = "";     // Nerd Font
pub const ICON_CONNECTED_FALLBACK: &str = "●";  // Unicode

pub const ICON_DISCONNECTED: &str = "";  // Nerd Font  
pub const ICON_DISCONNECTED_FALLBACK: &str = "○"; // Unicode

pub const ICON_USER: &str = "";
pub const ICON_USER_FALLBACK: &str = "👤";

pub const ICON_CHAT: &str = "";
pub const ICON_CHAT_FALLBACK: &str = "💬";

pub const ICON_SETTINGS: &str = "";
pub const ICON_SETTINGS_FALLBACK: &str = "⚙";

pub const ICON_WARNING: &str = "";
pub const ICON_WARNING_FALLBACK: &str = "⚠";

// Helper function
pub fn icon(nerd: &str, fallback: &str, use_nerd_font: bool) -> &str {
    if use_nerd_font { nerd } else { fallback }
}
```

## Testing Fonts

To test if a font is available on your system:

```rust
use iced::Font;

// System font by name
let font = Font::with_name("FiraCode Nerd Font");

// Embedded font
let font = Font::External {
    name: "My Custom Font",
    bytes: include_bytes!("../fonts/MyFont.ttf"),
};

// Use in text widgets
text("Icon text")
    .font(font)
    .size(16)
```

## Resources

- **Nerd Fonts**: https://www.nerdfonts.com/
- **Nerd Fonts Cheat Sheet**: https://www.nerdfonts.com/cheat-sheet
- **Material Icons**: https://fonts.google.com/icons
- **Unicode Symbols**: https://www.unicode.org/charts/
- **Iced Font Docs**: https://docs.rs/iced/latest/iced/font/

## License Considerations

- **Nerd Fonts**: MIT License (free to bundle)
- **Material Icons**: Apache License 2.0 (free to bundle)
- **Unicode Symbols**: No licensing needed (part of Unicode standard)

Make sure to include the font license file when bundling fonts with the application.