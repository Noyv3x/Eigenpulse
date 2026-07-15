(function () {
  "use strict";

  var SPEC_ATTRIBUTE = "data-ep-chart-spec";
  var SELECTOR = "[" + SPEC_ATTRIBUTE + "]";
  var RUNTIME_URL = "/static/vendor/eigenpulse-charts-6.1.0.js";
  var HYDRATED_EVENT = "eigenpulse:hydrated";
  var generations = new WeakMap();
  var runtimePromise;
  var runtime;

  function nextGeneration(element) {
    var generation = (generations.get(element) || 0) + 1;
    generations.set(element, generation);
    return generation;
  }

  function loadRuntime() {
    if (!runtimePromise) {
      runtimePromise = import(RUNTIME_URL)
        .then(function (loaded) {
          runtime = loaded;
          return loaded;
        })
        .catch(function (error) {
          runtimePromise = undefined;
          throw error;
        });
    }
    return runtimePromise;
  }

  function render(element) {
    if (!(element instanceof HTMLElement) || !element.isConnected) return;
    var raw = element.getAttribute(SPEC_ATTRIBUTE);
    if (!raw) {
      dispose(element);
      return;
    }
    var generation = nextGeneration(element);
    element.dataset.epChartState = "loading";

    loadRuntime()
      .then(function (loaded) {
        if (!element.isConnected || generations.get(element) !== generation) {
          loaded.dispose(element);
          return;
        }
        var spec = JSON.parse(element.getAttribute(SPEC_ATTRIBUTE) || "null");
        if (!spec || typeof spec !== "object") throw new TypeError("invalid chart specification");
        loaded.mountOrUpdate(element, spec);
        element.dataset.epChartState = "ready";
      })
      .catch(function (error) {
        if (generations.get(element) !== generation) return;
        element.dataset.epChartState = "error";
        // The adjacent native table remains usable. Keep runtime details out of
        // the DOM because imported-module errors can include deployment paths.
        console.error("Eigenpulse chart could not be rendered", error);
      });
  }

  function renderTree(node) {
    if (!(node instanceof Element)) return;
    if (node.matches(SELECTOR)) render(node);
    node.querySelectorAll(SELECTOR).forEach(render);
  }

  function dispose(element) {
    nextGeneration(element);
    if (runtime) runtime.dispose(element);
  }

  function disposeTree(node) {
    if (!(node instanceof Element)) return;
    if (node.matches(SELECTOR)) dispose(node);
    node.querySelectorAll(SELECTOR).forEach(dispose);
  }

  function refreshEnvironment() {
    if (runtime) runtime.refreshAll();
  }

  function start() {
    document.querySelectorAll(SELECTOR).forEach(render);
    var observer = new MutationObserver(function (records) {
      var refresh = false;
      records.forEach(function (record) {
        if (record.type === "attributes") {
          if (record.attributeName === SPEC_ATTRIBUTE) render(record.target);
          else refresh = true;
          return;
        }
        record.removedNodes.forEach(disposeTree);
        record.addedNodes.forEach(renderTree);
      });
      if (refresh) refreshEnvironment();
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: [SPEC_ATTRIBUTE, "data-theme", "data-density"],
      childList: true,
      subtree: true,
    });

    var reducedMotion = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)");
    if (reducedMotion) reducedMotion.addEventListener("change", refreshEnvironment);
  }

  // Never key this off DOMContentLoaded: the WASM module is dynamically
  // imported and may still be hydrating Tachys' SSR nodes at that point.
  // hydrate() sets a durable marker and emits an event, covering both script
  // execution orders without a timer or a race that replaces SSR children.
  var started = false;
  function startAfterHydration() {
    if (started) return;
    started = true;
    start();
  }
  window.addEventListener(HYDRATED_EVENT, startAfterHydration, { once: true });
  if (document.documentElement.getAttribute("data-ep-hydrated") === "true") {
    startAfterHydration();
  }

  window.EigenpulseChartLoader = Object.freeze({
    runtimeUrl: RUNTIME_URL,
    scan: function () {
      if (started) document.querySelectorAll(SELECTOR).forEach(render);
    },
  });
})();
