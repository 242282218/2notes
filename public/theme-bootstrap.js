/* global localStorage */
(function () {
  var storageKey = "2notes-theme-mode";
  var mode = "system";

  try {
    var saved = localStorage.getItem(storageKey);
    if (saved === "light" || saved === "dark" || saved === "system") {
      mode = saved;
    }
  } catch {
    mode = "system";
  }

  var theme = mode;
  if (mode === "system") {
    try {
      theme = window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
    } catch {
      theme = "light";
    }
  }

  var html = document.documentElement;
  html.setAttribute("data-theme", theme);
  html.style.colorScheme = theme;
})();
