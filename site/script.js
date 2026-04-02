(() => {
  const apiDocsLink = document.getElementById("apiDocsLink");
  const apiDocsHint = document.getElementById("apiDocsHint");

  if (!(apiDocsLink instanceof HTMLAnchorElement) || !(apiDocsHint instanceof HTMLElement)) return;

  // If `site/chaosfilter/index.html` exists and is served by a local server, it will resolve.
  // When opening from file://, fetch is usually blocked; we keep the hint visible.
  const apiIndex = new URL("./chaosfilter/index.html", window.location.href).toString();

  const canFetch =
    window.location.protocol === "http:" || window.location.protocol === "https:";

  if (!canFetch) {
    apiDocsHint.textContent =
      "Tip: run a local server (serve.ps1 / python http.server) to enable API docs detection.";
    return;
  }

  fetch(apiIndex, { method: "HEAD" })
    .then((res) => {
      if (!res.ok) return;
      apiDocsLink.hidden = false;
      apiDocsHint.remove();
    })
    .catch(() => {
      // keep hint
    });
})();
