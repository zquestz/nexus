# Voice Chat

This guide covers push-to-talk voice chat for channels and user messages.

## Overview

Nexus BBS supports real-time voice communication using:

- **Opus codec** — High-quality audio at low bandwidth
- **DTLS encryption** — Secure UDP transport
- **Push-to-talk** — Hold or toggle a key to transmit
- **WebRTC audio processing** — Noise suppression, echo cancellation, and automatic gain control

Voice chat works in both channels (group voice) and user messages (1-on-1 voice).

## Requirements

### Permissions

| Permission     | Required For                  |
| -------------- | ----------------------------- |
| `voice_listen` | Joining voice chat (required) |
| `voice_talk`   | Transmitting audio (optional) |

You must have `voice_listen` to join a voice session. Without `voice_talk`, you can listen but not speak.

### Audio Devices

- **Microphone** — Required to transmit (if you have `voice_talk`)
- **Speakers/Headphones** — Required to hear others

Configure audio devices in **Settings > Audio** before joining voice.

**Note:** Nexus automatically handles audio devices that don't natively support 48kHz (the sample rate required by the Opus codec). If your device uses a different sample rate (e.g., 44.1kHz or 96kHz), audio is automatically resampled with minimal latency impact.

### Network

- **Proxy** — Voice uses UDP, which can't be routed through a SOCKS5 proxy. When a proxy is active, voice is blocked by default. You can opt in via **Settings > Network > Allow Voice Bypass**, which sends voice directly to the server — **this exposes your real IP**, bypassing the proxy for voice traffic. See [Settings](11-settings.md).
- **Firewall** — UDP traffic on the server's BBS port (default 7500) must not be blocked.

## Joining Voice

### From a Channel Tab

1. Switch to a channel tab (e.g., `#general`)
2. Click the **microphone icon** (🎤) in the input bar
3. The voice bar appears above the input area when connected

### From a User Message Tab

1. Switch to a user message tab
2. Click the **microphone icon** (🎤) in the input bar
3. Voice starts when the other user also joins

If you do not already have a tab open, use `/focus nickname` to open the user
message tab first. `/focus` is available when either chat or voice is enabled,
so a voice-only session can still start direct voice.

**Note:** You cannot join voice from the Console tab.

### One Session at a Time

You can only be in one voice session at a time, even if connected to multiple servers. If you try to join voice while already in a session:

- You'll see an error message
- Leave the current voice session first

## Voice Bar

When in voice, a bar appears above the input area showing:

```
🎧 #general (3 in voice) │ 🎤 [▮▮▮▮░░░░] Alice
```

- **Headphones icon** — Indicates you're in voice
- **Target name** — Channel name or user nickname
- **Participant count** — How many people are in this voice session
- **Mic icon + VU meter** — When you're transmitting, shows your input level
- **Speaking names** — Names of others currently speaking

### VU Meter

When transmitting, a segmented VU meter appears next to the mic icon showing your input level in real-time:

- **Green segments** (0-60%) — Normal speaking level
- **Yellow segments** (60-80%) — Getting loud
- **Red segments** (80-100%) — Too hot / clipping

The meter updates in real time for smooth visual feedback. If you see red frequently, move back from your microphone or lower your system input volume.

The voice bar only appears on the connection with an active voice session.

## Push-to-Talk (PTT)

Voice transmission uses push-to-talk—you must press a key to transmit.

### PTT Modes

Configure in **Settings > Audio**:

| Mode       | Behavior                                                               |
| ---------- | ---------------------------------------------------------------------- |
| **Hold**   | Press and hold the key to talk; release to stop                        |
| **Toggle** | Press once to enable voice-activated transmission; press again to stop |

**Toggle mode with silence detection:** When you toggle on, your microphone becomes "hot" but only transmits when you're actually speaking. Background noise and silence are automatically filtered using audio level detection on the processed signal. A brief holdover period after speech prevents clipping word endings. This gives you hands-free operation while preventing constant transmission of ambient sound. Toggle off to fully mute.

### PTT Release Delay

Configure in **Settings > Audio > PTT Release Delay**:

| Setting   | Description                                                  |
| --------- | ------------------------------------------------------------ |
| **Off**   | Stop transmitting immediately when key is released (default) |
| **100ms** | Continue transmitting for 100ms after release                |
| **300ms** | Continue transmitting for 300ms after release                |
| **500ms** | Continue transmitting for 500ms after release                |

This prevents cutting off the end of words or sentences when you release the PTT key. The delay applies to both Hold and Toggle modes.

If you press PTT again during the delay period, the timer is cancelled and transmission continues normally.

### Default Key

The default PTT key is **backtick** (`` ` ``), also known as the grave or tilde key.

### Changing the PTT Key

1. Open **Settings > Audio**
2. Click the **PTT Key** field
3. Press your desired key (with optional modifiers)
4. Click **Save**

Supported keys include:

- Letter keys (A-Z)
- Number keys (0-9)
- Function keys (F1-F24)
- Special keys (Space, Tab, Backtick, etc.)

### Modifier Key Combinations

You can use modifier keys with your PTT key for combinations like:

- `Ctrl+Space`
- `Alt+F1`
- `Ctrl+Shift+A`
- `Cmd+Space` (macOS)

Supported modifiers:

- **Ctrl** (Control)
- **Alt**
- **Shift**
- **Super** / **Cmd** (Windows/Super key on Linux, Command on macOS)

The key display is platform-aware—macOS shows "Cmd" while Windows and Linux show "Super" for the same key.

### When PTT is Active

The key only activates PTT when:

- You're in a voice session
- The Nexus window doesn't need to be focused (global hotkey)

When not in voice, the key types normally.

## Speaking Indicators

### Your Own Status

When you're transmitting:

- Your PTT key is pressed (hold mode) or toggled on (toggle mode)
- Others in the session hear your audio

### Others Speaking

When someone else is speaking:

- Their name appears in the speaking indicator
- Audio plays through your speakers/headphones

### User List Icons

Voice participants show icons inline after their nickname in the user list (right panel):

- **🎧** — In voice but not speaking
- **🎤** — Currently speaking (highlighted in green)

## Mute All

You can mute all incoming voice audio while staying in the voice session:

1. Look for the **speaker icon** (🔊) on the right side of the voice bar
2. Click it to mute all incoming audio
3. The icon changes to **muted** (🔇) when active
4. Click again to unmute

This is useful when you need to temporarily stop hearing everyone without leaving the voice session. You can still transmit with PTT while muted.

## Muting Individual Users

You can mute individual users so you don't hear them:

1. Find the user in the user list
2. Click their name to open the action bar
3. Click the **mute** button

This is client-side only—they can still hear you, and others can still hear them.

To unmute, click the mute button again.

## Leaving Voice

### Click the Mic Button

Click the **microphone icon** (🎤) again to leave voice.

### Leave the Channel

If you leave a channel while in voice for that channel, you automatically leave voice too. You'll see a "You have left voice chat" message.

### Automatic Leave

Voice automatically ends when:

- You disconnect from the server
- The server restarts
- Your `voice_listen` permission is revoked
- You close the client

**Note:** If only your `voice_talk` permission is revoked, you remain in voice but can no longer transmit.

## Audio Settings

Configure voice in **Settings > Audio**:

| Setting                    | Description                                           |
| -------------------------- | ----------------------------------------------------- |
| **Output Device**          | Speakers/headphones for voice and notification sounds |
| **Input Device**           | Microphone for voice transmission                     |
| **Voice Quality**          | Audio quality/bandwidth tradeoff                      |
| **PTT Key**                | Key to press for push-to-talk                         |
| **PTT Mode**               | Hold or Toggle                                        |
| **PTT Release Delay**      | Continue transmitting briefly after releasing PTT key |
| **Noise Suppression**      | Reduce background noise from your microphone          |
| **Echo Cancellation**      | Remove speaker audio from your microphone signal      |
| **Automatic Gain Control** | Automatically adjust microphone volume                |

### Voice Quality Levels

| Level     | Bitrate | Best For                   |
| --------- | ------- | -------------------------- |
| Low       | 16 kbps | Poor connections           |
| Medium    | 32 kbps | Moderate connections       |
| High      | 64 kbps | Good connections (default) |
| Very High | 96 kbps | Excellent connections      |

Higher quality uses more bandwidth but sounds better.

**Note:** Quality changes apply immediately—you don't need to leave and rejoin voice. If you're experiencing audio issues, try lowering the quality while in the call.

### Audio Processing

Nexus uses the same audio processing technology as Discord, Google Meet, and other professional voice applications (WebRTC AudioProcessing).

| Feature                      | Default  | Description                                                       |
| ---------------------------- | -------- | ----------------------------------------------------------------- |
| **Microphone Boost**         | Off      | Pre-gain for quiet mics: Off, +6 dB, +12 dB, or +18 dB            |
| **Noise Suppression**        | Moderate | Off, Low, Moderate, High, or Very High background noise filtering |
| **Echo Cancellation**        | Off      | Removes speaker audio picked up by your microphone                |
| **Automatic Gain Control**   | On       | Normalizes your volume so you're not too quiet or too loud        |
| **Keyboard Noise Reduction** | Off      | Suppresses transient sounds like keyboard clicks and mouse clicks |

**Microphone Boost** amplifies your mic signal before any processing. Use it if your microphone is too quiet for Automatic Gain Control to bring to usable levels. Each step doubles the amplification (+6 dB = 2×, +12 dB = 4×, +18 dB = 8×).

**Noise Suppression** has five levels. Higher levels remove more background noise but may introduce slight speech distortion. Moderate is a good balance for most environments. Use High or Very High in noisy locations like cafes or open offices.

**Why is echo cancellation off by default?** Most users wear headphones, which don't cause echo. Echo cancellation adds processing overhead and is only needed when using speakers. Enable it if others hear themselves echoing back.

**Why is keyboard noise reduction off by default?** Transient suppression can occasionally clip the start of words. Enable it if you type while talking and want to reduce keyboard noise for others.

All audio processing settings apply immediately—you don't need to leave and rejoin voice.

**Toggle PTT mode:** In Toggle mode, the microphone stays open after pressing the PTT key. Audio processing (noise suppression, AGC) still applies to keep transmitted audio clean, and silence detection automatically suppresses transmission when you're not speaking.

### Testing Your Microphone

1. Open **Settings > Audio**
2. Select your input device
3. Click **Test Microphone**
4. The VU meter shows your microphone input level in real-time (green/yellow/red segments)
5. Speak to verify the meter responds

The same VU meter style is used in both the settings mic test and the voice bar during transmission.

## Troubleshooting

### Can't Join Voice

**"You don't have permission"**

- Contact the server admin to grant `voice_listen`

**"Already in voice on another connection"**

- Leave voice on your other server connection first

**"Not in channel"**

- Join the channel before trying to join voice

**"Voice is blocked while using a proxy"**

- Voice uses UDP, which cannot be routed through SOCKS5 proxies, so it's blocked by default when a proxy is active
- Either disable the proxy in **Settings > Network**, or enable **Allow Voice Bypass** there to connect voice directly — note this exposes your real IP to the server

### No Audio Output

1. Check **Settings > Audio > Output Device** is correct
2. Check your system volume isn't muted
3. Try selecting a different output device
4. Restart the client if you changed devices while in voice

### Microphone Not Working

1. Check **Settings > Audio > Input Device** is correct
2. Verify the mic level meter responds when you speak
3. Check your operating system's microphone permissions
4. Ensure no other application is using the microphone exclusively

### Audio Quality Issues

**Choppy or robotic audio:**

- Lower the voice quality setting
- Check your network connection
- The speaker may have a poor connection

**Echo or feedback:**

- Enable **Echo Cancellation** in Settings > Audio
- Use headphones instead of speakers
- Move microphone away from speakers

**Too quiet or too loud:**

- Enable **Automatic Gain Control** in Settings > Audio (on by default)
- Adjust your system microphone volume
- Ask others to adjust their system volume

**Background noise:**

- Enable **Noise Suppression** in Settings > Audio (on by default)
- Move away from noise sources (fans, AC, keyboards)
- Use a directional microphone or headset

### PTT Key Not Working

1. Verify you're in a voice session (voice bar is visible)
2. Check **Settings > Audio > PTT Key** is set correctly
3. Try a different key (some keys may be captured by other applications)
4. On Linux, ensure your display server allows global hotkeys
5. On Windows, PTT won't work in applications running with administrator privileges unless Nexus is also run as administrator

### Connection Failed

**"DTLS handshake failed"**

- The server may not support voice chat
- Check your firewall allows UDP on the server's port
- Try reconnecting

**"Connection timeout"**

- Network issues between you and the server
- Try again or check your connection

## Technical Details

### Protocol

- **Signaling:** TCP (same connection as chat)
- **Audio:** UDP with DTLS encryption
- **Codec:** Opus at 48kHz mono
- **Frame size:** 10ms (480 samples per frame)
- **Audio processing:** WebRTC AudioProcessing 2.0 (same as Discord, Chrome, Meet)
- **Resampling:** Automatic via rubato (FFT-based) for non-48kHz devices

### Audio Processing Pipeline

Nexus uses WebRTC AudioProcessing 2.0 to enhance voice quality. Audio flows through two paths:

**Capture path** (microphone → network):

1. **Capture** — cpal reads 10ms frames from the microphone (resampled to 48kHz if needed)
2. **High-pass filter** — Removes DC offset and sub-bass rumble (always on)
3. **Noise suppression** — Removes steady-state background noise (fans, AC). Moderate level balances suppression vs. speech distortion.
4. **Echo cancellation** — AEC3 removes speaker audio picked up by the microphone. Uses render path analysis as reference. Auto-estimates delay.
5. **Transient suppression** — Reduces keyboard clicks, mouse clicks, and other sudden noises. PTT key events are signaled to improve detection accuracy.
6. **Automatic gain control** — GainController2 with adaptive digital gain normalizes volume to consistent levels.
7. **Opus encode** — Compressed and sent via DTLS/UDP.

**Render path** (network → speakers):

1. **DTLS/UDP receive** — Encrypted voice packets arrive from the server.
2. **Opus decode** — Decompressed to 48kHz PCM. Packet loss concealment (PLC) fills gaps.
3. **Jitter buffer** — Adaptive buffering (20-200ms) smooths network timing variations.
4. **AEC reference analysis** — Each decoded frame is analyzed (read-only) so the echo canceller knows what audio is being played back. This is critical for AEC to work.
5. **Mixing** — Multiple speakers are mixed together. Muted users and deafened state are handled here.
6. **Playback** — cpal writes to the output device (resampled from 48kHz if needed).

**Processor hints** — The processor receives runtime hints to improve quality:

| Hint              | When                 | Effect                                                                    |
| ----------------- | -------------------- | ------------------------------------------------------------------------- |
| Output muted      | User deafens         | AEC skips echo cancellation (no speaker output = no echo)                 |
| Key pressed       | PTT key down/up      | Transient suppressor identifies keyboard sounds more accurately           |
| Linear AEC output | AEC + NS both active | Noise suppressor analyzes AEC's linear output for better noise estimation |

All processing happens in 10ms frames (480 samples at 48kHz). Settings changes apply immediately without restarting the voice session.

### Bandwidth Usage

Approximate bandwidth per direction:

| Quality   | Bandwidth |
| --------- | --------- |
| Low       | ~20 kbps  |
| Medium    | ~40 kbps  |
| High      | ~75 kbps  |
| Very High | ~110 kbps |

Actual usage includes packet overhead.

### Latency

Typical voice latency: 40-100ms depending on:

- Network latency to server
- Jitter buffer size (20-200ms adaptive)
- Audio device latency
- Resampling (adds ~10-20ms if device doesn't support 48kHz)

## Next Steps

- [Settings](11-settings.md) — Configure audio and other preferences
- [Chat](03-chat.md) — Text chat in channels and user messages
