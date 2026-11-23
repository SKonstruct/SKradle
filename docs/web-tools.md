# Websites on the Tools page

The Tools page lists two Spiral Knights websites that open inside SKradle: the Auction House (sk-ah.com) and Kozmas
(kozmas.gg). They are other people's sites and not part of SKradle or the game. Each has an **Open** button (inside the
launcher) and a globe button (your browser).

## Sign-in and data are kept

Both sites have accounts, so the embedded view must remember cookies and local storage between runs. It does, because
SKradle uses the system's own web view and each one stores its data on disk by default:

| System | Web view | Where the data lives |
| --- | --- | --- |
| macOS | WebKit (`WKWebView`) | WebKit's persistent website data store of the app |
| Windows | WebView2 | `WebView2` inside the app's local data folder (`%LOCALAPPDATA%`, SKradle's own folder) |
| Linux | none (Qt ships no web view add-on for it) | the sites open in your browser, which keeps your sign-in as usual |

Nothing is shared with the player's browser, and nothing is read by SKradle: it never sees the pages' cookies or
passwords. To sign out, use the sites' own sign-out. To clear what a site stored, remove the data folder above (macOS:
clear SKradle's website data in the system's settings). The server's own web interface (Servers page) uses the same
web view; cookies are kept per site, so the sites cannot read each other's.

Qt's own source confirms the behaviour: the macOS backend only switches to a non-persistent store when asked, and the
Windows backend creates its WebView2 environment with a user-data folder under `AppLocalDataLocation`.
