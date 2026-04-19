// Scale content to fit viewport when DPI scaling causes overflow.
// Extracted from an inline <script> block so CSP script-src can drop
// 'unsafe-inline' (PRD 3.1.12).
(function () {
  const DESIGN_WIDTH = 1282;
  const DESIGN_HEIGHT = 759;

  function updateZoom() {
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    const scaleX = vw / DESIGN_WIDTH;
    const scaleY = vh / DESIGN_HEIGHT;
    const scale = Math.min(scaleX, scaleY, 1);

    if (scale < 1) {
      document.documentElement.style.zoom = scale;
    } else {
      document.documentElement.style.zoom = '';
    }
  }

  updateZoom();
  window.addEventListener('resize', updateZoom);
})();
