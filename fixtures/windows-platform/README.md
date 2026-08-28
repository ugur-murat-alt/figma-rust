# Windows platform qualification fixture

`windows-platform-probe` opens a real GPUI window containing Segoe UI text and a
full-window click target. The Windows workflow changes the single active display
to 100%, 125%, or 150% with the checked DisplayConfig helper, then proves the
effective window DPI with the documented `GetDpiForWindow` API.

For each scale, two separately launched windows must produce byte-identical client
captures and both must observe a Win32 primary-click message through GPUI. The
evidence report retains logical and physical dimensions, image hash, Segoe UI file
hash, GPU identity, input result, GPUI revision, and applied DPI profile. The
checked geometry file describes the logical 160x80 probe root; physical pixels are
expected to be 160x80, 200x100, and 240x120 respectively.

The DPI helper is adapted from the public-domain `lihas/windows-DPI-scaling-sample`
at commit `738ac18b7a7ce2d8fdc157eb825de9cb5eee0448`. DisplayConfig's DPI packet types
are reverse-engineered, so the workflow validates packet sizes, requires exactly
one active display, reads the applied profile back, and fails without publishing a
passing report if Windows rejects or changes that contract.
