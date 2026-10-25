// Tiza docs: theme toggle, navigation drawer, search, copy buttons, active TOC entry.
// Own code. The search uses the elasticlunr index that Zola writes (search_index.en.js).
(function () {
  "use strict";
  var base = document.currentScript.getAttribute("data-base") || "/";
  if (base.slice(-1) !== "/") base += "/";
  var root = document.documentElement;

  // ---- Theme -------------------------------------------------------------
  function applyTheme(t) {
    root.setAttribute("data-theme", t);
    document.getElementById("hl-light").media = t === "light" ? "all" : "not all";
    document.getElementById("hl-dark").media = t === "dark" ? "all" : "not all";
    var btn = document.querySelector(".theme-toggle");
    if (btn) btn.setAttribute("aria-label", t === "dark" ? "Switch to the light theme" : "Switch to the dark theme");
  }
  function stored() {
    try { return localStorage.getItem("tiza-theme"); } catch (e) { return null; }
  }
  applyTheme(root.getAttribute("data-theme") || "light");
  document.querySelector(".theme-toggle").addEventListener("click", function () {
    var t = root.getAttribute("data-theme") === "dark" ? "light" : "dark";
    try { localStorage.setItem("tiza-theme", t); } catch (e) {}
    applyTheme(t);
  });
  // With no stored choice, follow a change of the system setting.
  var mq = window.matchMedia("(prefers-color-scheme: dark)");
  var onSystem = function (e) { if (!stored()) applyTheme(e.matches ? "dark" : "light"); };
  if (mq.addEventListener) mq.addEventListener("change", onSystem);

  // ---- Navigation drawer (narrow screens) --------------------------------
  var navBtn = document.querySelector(".nav-toggle");
  var scrim = document.querySelector(".nav-scrim");
  function setNav(open) {
    document.body.classList.toggle("nav-open", open);
    navBtn.setAttribute("aria-expanded", String(open));
    navBtn.setAttribute("aria-label", open ? "Close the navigation" : "Open the navigation");
    scrim.hidden = !open;
  }
  navBtn.addEventListener("click", function () { setNav(!document.body.classList.contains("nav-open")); });
  scrim.addEventListener("click", function () { setNav(false); });
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape" && document.body.classList.contains("nav-open")) setNav(false);
  });
  var current = document.querySelector('.sidebar [aria-current="page"]');
  if (current && current.scrollIntoView) current.scrollIntoView({ block: "nearest" });

  // ---- Heading anchors ---------------------------------------------------
  document.querySelectorAll(".doc :is(h2, h3, h4)[id]").forEach(function (h) {
    var a = document.createElement("a");
    a.className = "anchor";
    a.href = "#" + h.id;
    a.setAttribute("aria-label", "Link to this section");
    a.textContent = "#";
    h.appendChild(document.createTextNode(" "));
    h.appendChild(a);
  });

  // ---- Blocks of `:` commands wrap -----------------------------------------
  // A block whose every line is a `:` command wraps a long line with a
  // hanging indent, so a narrow screen shows the whole command. Other blocks
  // (text exports, JSON, shell) keep their lines and scroll.
  document.querySelectorAll(".doc pre > code").forEach(function (code) {
    var text = code.textContent.replace(/\n$/, "");
    var lines = text.split("\n");
    if (!lines.length || !lines.every(function (l) { return /^:/.test(l); })) return;
    code.parentNode.classList.add("cmds");
    code.parentNode.dataset.text = text;
    code.textContent = "";
    lines.forEach(function (l) {
      var s = document.createElement("span");
      s.className = "cmd";
      s.textContent = l;
      code.appendChild(s);
    });
  });

  // ---- Copy buttons on code blocks ---------------------------------------
  document.querySelectorAll(".doc pre").forEach(function (pre) {
    if (!navigator.clipboard) return;
    var b = document.createElement("button");
    b.type = "button";
    b.className = "copy-btn";
    b.textContent = "Copy";
    b.addEventListener("click", function () {
      var code = pre.querySelector("code") || pre;
      var text = pre.dataset.text || code.innerText.replace(/\n$/, "");
      navigator.clipboard.writeText(text).then(function () {
        b.textContent = "Copied";
        setTimeout(function () { b.textContent = "Copy"; }, 1500);
      });
    });
    pre.appendChild(b);
  });

  // ---- Active entry in "On this page" ------------------------------------
  var tocLinks = Array.prototype.slice.call(document.querySelectorAll(".toc a"));
  if (tocLinks.length && "IntersectionObserver" in window) {
    var byId = {};
    tocLinks.forEach(function (a) { byId[decodeURIComponent(a.hash.slice(1))] = a; });
    var visible = {};
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) { visible[en.target.id] = en.isIntersecting; });
      var first = null;
      document.querySelectorAll(".doc h2[id], .doc h3[id]").forEach(function (h) {
        if (!first && visible[h.id]) first = h.id;
      });
      if (first) {
        tocLinks.forEach(function (a) { a.classList.remove("active"); });
        if (byId[first]) byId[first].classList.add("active");
      }
    }, { rootMargin: "-60px 0px -60% 0px" });
    document.querySelectorAll(".doc h2[id], .doc h3[id]").forEach(function (h) { io.observe(h); });
  }

  // ---- Search ------------------------------------------------------------
  var input = document.getElementById("search-input");
  var box = document.getElementById("search-results");
  var index = null;
  var loading = null;
  var selected = -1;

  // The navigation knows the group of each page.
  var groupOf = {};
  document.querySelectorAll(".nav-group").forEach(function (g) {
    var title = g.querySelector("summary").textContent.trim();
    g.querySelectorAll("a").forEach(function (a) { groupOf[a.pathname] = title; });
  });

  function loadScript(src) {
    return new Promise(function (resolve, reject) {
      var s = document.createElement("script");
      s.src = src;
      s.onload = resolve;
      s.onerror = function () { reject(new Error("cannot load " + src)); };
      document.head.appendChild(s);
    });
  }
  function loadIndex() {
    if (!loading) {
      loading = loadScript(base + "elasticlunr.min.js")
        .then(function () { return loadScript(base + "search_index.en.js"); })
        .then(function () { index = elasticlunr.Index.load(window.searchIndex); return index; });
    }
    return loading;
  }
  function esc(s) {
    return s.replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }
  // Zola writes <, > and & of the indexed text as entities. Decode them, so
  // that esc() does not escape them twice and a query with < or & matches.
  function unent(s) {
    return s.replace(/&(amp|lt|gt|quot|#39|#x27);/g, function (_, e) {
      return { amp: "&", lt: "<", gt: ">", quot: '"', "#39": "'", "#x27": "'" }[e];
    });
  }
  function terms(q) {
    return q.toLowerCase().split(/\s+/).filter(function (t) { return t.length > 1; });
  }
  // Find the matches on the raw text with one regex for all the terms, then
  // escape each part. A term never matches inside <mark> or an entity.
  function highlight(text, ts) {
    if (!ts.length) return esc(text);
    var alt = ts.slice().sort(function (a, b) { return b.length - a.length; })
      .map(function (t) { return t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"); }).join("|");
    var re = new RegExp(alt, "gi");
    var out = "", last = 0, m;
    while ((m = re.exec(text)) !== null) {
      out += esc(text.slice(last, m.index)) + "<mark>" + esc(m[0]) + "</mark>";
      last = m.index + m[0].length;
    }
    return out + esc(text.slice(last));
  }
  function snippet(body, ts) {
    var lower = body.toLowerCase();
    var at = -1;
    for (var i = 0; i < ts.length && at < 0; i++) at = lower.indexOf(ts[i]);
    var start = Math.max(0, at - 60);
    if (start > 0) { var sp = body.indexOf(" ", start); if (sp > 0 && sp < at) start = sp + 1; }
    var s = body.slice(start, start + 170).replace(/\s+/g, " ").trim();
    return (start > 0 ? "… " : "") + s + (start + 170 < body.length ? " …" : "");
  }
  function render(q) {
    var ts = terms(q);
    var hits = index.search(q, {
      bool: "AND",
      expand: true,
      fields: { title: { boost: 3 }, body: { boost: 1 } },
    }).slice(0, 10);
    selected = -1;
    if (!hits.length) {
      box.innerHTML = '<p class="r-empty">No page contains “' + esc(q) + "”.</p>";
    } else {
      box.innerHTML = "<ol>" + hits.map(function (h) {
        var doc = index.documentStore.getDoc(h.ref);
        var path = new URL(h.ref, location.href).pathname;
        var group = groupOf[path] || "";
        return '<li><a href="' + esc(h.ref) + '"><span class="r-title">' + highlight(unent(doc.title || path), ts) +
          (group ? '<span class="r-section">' + esc(group) + "</span>" : "") + "</span>" +
          '<span class="r-snippet">' + highlight(snippet(unent(doc.body || ""), ts), ts) + "</span></a></li>";
      }).join("") + "</ol>";
    }
    box.hidden = false;
  }
  function run() {
    var q = input.value.trim();
    if (q.length < 2) { box.hidden = true; return; }
    loadIndex().then(function () { render(q); }, function () {
      box.innerHTML = '<p class="r-empty">The search index did not load.</p>';
      box.hidden = false;
    });
  }
  function move(d) {
    var links = box.querySelectorAll("a");
    if (!links.length) return;
    if (selected >= 0) links[selected].removeAttribute("aria-selected");
    selected = (selected + d + links.length) % links.length;
    links[selected].setAttribute("aria-selected", "true");
    links[selected].scrollIntoView({ block: "nearest" });
  }
  var timer;
  input.addEventListener("input", function () { clearTimeout(timer); timer = setTimeout(run, 80); });
  input.addEventListener("focus", function () { loadIndex(); if (input.value.trim().length > 1) run(); });
  input.addEventListener("keydown", function (e) {
    if (e.key === "ArrowDown") { e.preventDefault(); move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(-1); }
    else if (e.key === "Enter") {
      var links = box.querySelectorAll("a");
      var a = links[selected >= 0 ? selected : 0];
      if (a) location.href = a.href;
    } else if (e.key === "Escape") { box.hidden = true; input.blur(); }
  });
  document.addEventListener("keydown", function (e) {
    var tag = (document.activeElement && document.activeElement.tagName) || "";
    if (e.key === "/" && tag !== "INPUT" && tag !== "TEXTAREA") { e.preventDefault(); input.focus(); }
  });
  document.addEventListener("click", function (e) {
    if (!e.target.closest(".search")) box.hidden = true;
  });
  // A link can open the search: ?q=autosave
  var q0 = new URLSearchParams(location.search).get("q");
  if (q0) { input.value = q0; input.focus(); run(); }
})();
