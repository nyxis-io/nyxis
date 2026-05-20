(function () {
  try {
    if (localStorage.getItem("nxs-theme") === "light") {
      document.documentElement.dataset.theme = "light";
    }
  } catch (_) {}
})();
