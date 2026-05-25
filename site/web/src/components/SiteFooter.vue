<template>
  <footer class="site-footer">
    <div class="footer-brand">
      <span class="footer-wordmark">Nyxis</span>
      <span class="footer-tagline">Bi-modal NXS — human <code>.nxs</code>, binary <code>.nxb</code></span>
    </div>
    <div class="links">
      <a href="https://github.com/nyxis-io/nyxis">Open core</a>
      <a href="https://github.com/nyxis-io/nyxis-drivers">Drivers (MIT)</a>
      <RouterLink to="/demo/">Demos</RouterLink>
      <RouterLink to="/pricing/">Pricing</RouterLink>
      <a href="https://github.com/nyxis-io/nyxis/blob/main/SPEC.md" rel="noopener">Specification</a>
    </div>
  </footer>
</template>

<style scoped>
.footer-brand {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.footer-wordmark {
  font-family: var(--font-display);
  font-size: 15px;
  font-weight: 700;
  color: var(--text);
  letter-spacing: -0.02em;
}

.footer-tagline {
  font-size: 13px;
  color: var(--muted);
  max-width: 22rem;
  line-height: 1.45;
}

.footer-tagline code {
  font-family: var(--font-mono);
  font-size: 0.9em;
}
</style>
