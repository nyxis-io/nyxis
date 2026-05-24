const KEY = "nxs-theme";

function labelFor(theme: "light" | "dark") {
  return theme === "light" ? "Switch to dark theme" : "Switch to light theme";
}

export function readStoredTheme(): "light" | "dark" {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "dark";
  } catch {
    return "light";
  }
}

export function applyTheme(theme: "light" | "dark", persist = true) {
  if (theme === "light") {
    document.documentElement.dataset.theme = "light";
  } else {
    document.documentElement.removeAttribute("data-theme");
  }
  if (persist) {
    try {
      localStorage.setItem(KEY, theme);
    } catch {
      /* ignore */
    }
  }
  const short = theme === "light" ? "Dark" : "Light";
  document.querySelectorAll(".theme-toggle").forEach((btn) => {
    btn.setAttribute("aria-label", labelFor(theme));
    btn.setAttribute("title", labelFor(theme));
    btn.textContent = short;
  });
}

export function currentTheme(): "light" | "dark" {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

export function toggleTheme() {
  applyTheme(currentTheme() === "light" ? "dark" : "light", true);
}

/** Run before Vue paints (matches legacy theme-init.js). */
export function initTheme() {
  try {
    if (localStorage.getItem(KEY) === "light") {
      document.documentElement.dataset.theme = "light";
    }
  } catch {
    /* ignore */
  }
}

export function bindThemeToggle(el: HTMLElement) {
  el.addEventListener("click", toggleTheme);
  applyTheme(readStoredTheme(), false);
}
