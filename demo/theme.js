(function () {
  var KEY = "nxs-theme";

  function labelFor(theme) {
    return theme === "light" ? "Switch to dark theme" : "Switch to light theme";
  }

  function readStored() {
    try {
      var v = localStorage.getItem(KEY);
      return v === "light" || v === "dark" ? v : "dark";
    } catch (_) {
      return "dark";
    }
  }

  function apply(theme, persist) {
    if (theme === "light") {
      document.documentElement.dataset.theme = "light";
    } else {
      document.documentElement.removeAttribute("data-theme");
    }
    if (persist !== false) {
      try {
        localStorage.setItem(KEY, theme);
      } catch (_) {}
    }
    var short = theme === "light" ? "Dark" : "Light";
    document.querySelectorAll(".theme-toggle").forEach(function (btn) {
      btn.setAttribute("aria-label", labelFor(theme));
      btn.setAttribute("title", labelFor(theme));
      btn.textContent = short;
    });
  }

  function currentFromDom() {
    return document.documentElement.dataset.theme === "light" ? "light" : "dark";
  }

  function toggle() {
    apply(currentFromDom() === "light" ? "dark" : "light", true);
  }

  document.addEventListener("DOMContentLoaded", function () {
    document.querySelectorAll(".theme-toggle").forEach(function (btn) {
      btn.addEventListener("click", toggle);
    });
    apply(readStored(), false);
  });
})();
