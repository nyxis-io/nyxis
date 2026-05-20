(function () {
  var GLOBAL = [
    { id: "home", label: "Home", href: "/" },
    { id: "use-cases", label: "Use cases", href: "/use-cases/" },
    { id: "demo", label: "Demo", href: "/demo/" },
    { id: "bench", label: "Benchmark", href: "/bench/" },
    {
      id: "github",
      label: "GitHub",
      href: "https://github.com/nyxis-io/nyxis",
      external: true,
    },
  ];

  var DEMO_TOOLS = [
    { id: "demo", label: "All demos", href: "/demo/" },
    { id: "ticker", label: "Ticker", href: "/demo/ticker.html" },
    { id: "workers", label: "Workers", href: "/demo/workers.html" },
    { id: "explorer", label: "Explorer", href: "/demo/explorer.html" },
    { id: "wal", label: "WAL", href: "/demo/wal.html" },
  ];

  function esc(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/"/g, "&quot;");
  }

  function link(item, current) {
    var cur = current === item.id ? ' aria-current="page"' : "";
    var rel = item.external ? ' rel="noopener"' : "";
    var target = item.external ? ' target="_blank"' : "";
    return (
      '<a href="' +
      esc(item.href) +
      '"' +
      cur +
      rel +
      target +
      ">" +
      esc(item.label) +
      "</a>"
    );
  }

  function mount() {
    var root = document.getElementById("site-nav-root");
    if (!root) return;

    var current = document.body.getAttribute("data-nav-current") || "";
    var parts = [
      '<nav class="site-nav">',
      '<a class="home" href="/">Nyxis</a>',
      '<span class="sep">›</span>',
    ];
    GLOBAL.forEach(function (item) {
      parts.push(link(item, current));
    });
    parts.push(
      '<button type="button" class="theme-toggle" aria-label="Theme"></button>',
      "</nav>"
    );

    if (document.body.getAttribute("data-demo-subnav") === "true") {
      parts.push('<nav class="demo-subnav" aria-label="Demo pages">');
      DEMO_TOOLS.forEach(function (item) {
        parts.push(link(item, current));
      });
      parts.push("</nav>");
    }

    root.innerHTML = parts.join("");
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", mount);
  } else {
    mount();
  }
})();
