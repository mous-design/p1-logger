import { useEffect, useState } from "react";
import "./App.scss";
import { Dashboard } from "./components/Dashboard";
import { DetailView } from "./components/DetailView";
import { detailPath, parseRoute, type Route } from "./url";
import type { Metric } from "./types";

/** Owns which top-level page is showing, kept in sync with the URL (not
 * just in-memory state) -- a reload or a shared link should land back on
 * the same page instead of always resetting to the dashboard. */
function App() {
  const [route, setRoute] = useState<Route>(() => parseRoute(window.location.pathname));

  // Browser back/forward -- popstate fires on those, not on our own
  // pushState calls below, so this only needs to re-sync from the URL.
  useEffect(() => {
    function onPopState() {
      setRoute(parseRoute(window.location.pathname));
    }
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  function openDetail(metric: Metric) {
    window.history.pushState(null, "", detailPath(metric));
    setRoute({ kind: "detail", metric });
  }

  function backToDashboard() {
    window.history.pushState(null, "", "/");
    setRoute({ kind: "dashboard" });
  }

  if (route.kind === "detail") {
    // key={route.metric} forces a fresh DetailView (and its own fresh
    // period/anchor state) if the metric ever changes without the
    // component unmounting first -- e.g. navigating browser history
    // between two different /detail/<metric> entries.
    return <DetailView key={route.metric} initialMetric={route.metric} onBack={backToDashboard} />;
  }
  return <Dashboard onOpenDetail={openDetail} />;
}

export default App;
