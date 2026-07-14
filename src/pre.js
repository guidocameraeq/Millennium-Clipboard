// Millennium Clipboard — pre.js
//
// Runs BEFORE styles.css applies + before <body> renders, so the class is
// already on <html> by the time CSS first paints. Moved out of an inline
// <script> in index.html (Fase 3, Tarea 3.2) so a strict CSP with
// `script-src 'self'` — which blocks inline scripts — doesn't break it.
// Loaded as `<script src="pre.js"></script>` (parser-blocking) placed BEFORE
// the styles.css <link>, so it still runs first.
//
// This is a belt-and-suspenders fallback in case the @media query parser in
// the Android WebView trips over the combined "max-width OR pointer:coarse"
// syntax (seen on some Samsung builds). The class triggers the
// `html.is-mobile` rules at the bottom of styles.css (which use !important to
// beat the desktop defaults).
(function () {
  try {
    var touch = window.matchMedia && window.matchMedia('(pointer: coarse)').matches;
    var narrow = window.innerWidth && window.innerWidth <= 900;
    var androidUA = /android/i.test(navigator.userAgent || '');
    if (touch || narrow || androidUA) {
      document.documentElement.classList.add('is-mobile');
    }
  } catch (_) {}
  // FX preference must land on <html> before styles.css applies, otherwise
  // the grid/scanline flash for one frame on every boot.
  try {
    if (localStorage.getItem('fx') === 'off') {
      document.documentElement.classList.add('fx-off');
    }
  } catch (_) {}
})();
