// Download links from the latest GitHub release, and the main button for the visitor's system.
// Without JavaScript or when GitHub does not answer, every link leads to the release page.
(() => {
  const REPO = "LeoAlecksey/OpsOpsAi";
  const RELEASES = `https://github.com/${REPO}/releases/latest`;

  /** Which row of the download list fits this visitor. */
  function detectOs() {
    const ua = navigator.userAgent;
    // phones and tablets: there is no app for them, keep the generic button
    if (/Android|iPhone|iPad|iPod/i.test(ua)) return null;
    const platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    if (/Win/i.test(platform) || /Windows/i.test(ua)) return "win";
    if (/Mac/i.test(platform) || /Mac OS X/i.test(ua)) return "mac";
    if (/Linux/i.test(platform) || /Linux/i.test(ua)) return /aarch64|arm64|armv8/i.test(ua + platform) ? "linux-arm" : "linux";
    return null;
  }

  /** Release assets sorted into the list rows: [main file, extra files]. */
  function pick(assets) {
    const find = (re) => assets.find((a) => re.test(a.name));
    return {
      mac: [find(/aarch64\.dmg$/i) || find(/\.dmg$/i)],
      win: [find(/x64-setup\.exe$/i), find(/x64.*\.msi$/i)],
      linux: [find(/amd64\.AppImage$/i), find(/amd64\.deb$/i), find(/x86_64\.rpm$/i)],
      "linux-arm": [find(/aarch64\.AppImage$/i), find(/arm64\.deb$/i), find(/aarch64\.rpm$/i)],
    };
  }

  const label = (name) => {
    if (/setup\.exe$/i.test(name)) return "установщик .exe";
    const ext = name.match(/\.(dmg|msi|AppImage|deb|rpm)$/i);
    return ext ? `.${ext[1]}` : name;
  };
  const mb = (bytes) => `${Math.round(bytes / 1048576)} МБ`;
  const NAMES = { mac: "для macOS", win: "для Windows", linux: "для Linux", "linux-arm": "для Linux ARM64" };

  function render(release) {
    const rows = pick(release.assets || []);
    const version = (release.tag_name || "").replace(/^v/, "");
    const os = detectOs();
    if (version) document.getElementById("release").textContent = `Ранний доступ, версия ${version}`;

    for (const li of document.querySelectorAll("#dl-list li")) {
      const files = (rows[li.dataset.os] || []).filter(Boolean);
      if (!files.length) continue;
      const links = document.createElement("span");
      links.className = "dl-more";
      for (const f of files) {
        const a = document.createElement("a");
        a.href = f.browser_download_url;
        a.textContent = label(f.name);
        a.title = `${f.name}, ${mb(f.size)}`;
        links.append(a);
      }
      li.querySelector("a").replaceWith(links);
      if (li.dataset.os === os) li.classList.add("current");
    }

    const main = os && rows[os] && rows[os][0];
    if (main) {
      const btn = document.getElementById("dl-main");
      btn.href = main.browser_download_url;
      btn.textContent = `Скачать ${NAMES[os]}`;
      btn.title = `${main.name}, ${mb(main.size)}`;
    }
  }

  fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
    .then(render)
    .catch(() => {
      // no published release yet or GitHub is unreachable: the static links to the release page stay
      document.getElementById("dl-main").href = RELEASES;
      document.getElementById("dl-note").textContent = "Список файлов не загрузился: все сборки есть на странице релиза на GitHub.";
    });
})();
