# Security

Utter handles your microphone, what you dictate, your API keys and, with the
fn key option, a view of your key presses. Please report anything that could
expose any of these **privately**, through
[GitHub's private advisory form](https://github.com/ShanTirmizi/utter/security/advisories/new),
not as a public issue.

What Utter is meant to guarantee, so you can tell when it doesn't:

- Audio never leaves the Mac.
- API keys live only in the macOS Keychain, are sent only to their own
  provider, and are never written to a file or a log.
- Text goes only to the cleanup provider the user chose, and only when cleanup
  is set up.
- History is a file on the Mac readable only by the user.
- The fn key tap never records which keys are pressed.
