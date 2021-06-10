import * as svelteStore from "svelte/store";

import * as error from "ui/src/error";
import * as screen from "ui/src/screen";

import { Route, LoadedRoute, loadRoute, routeToPath } from "./definition";
export * from "./definition";

// TODO remove this and have users import the `unreachable` module
// directly
export { unreachable } from "ui/src/unreachable";

// This is only respected by Safari.
const DOCUMENT_TITLE = "Radicle Upstream";

const DEFAULT_ROUTE: Route = { type: "profile", activeTab: "projects" };
const INITIAL_ROUTE: LoadedRoute = { type: "loading" };

const historyStore: svelteStore.Writable<Route[]> = svelteStore.writable([
  DEFAULT_ROUTE,
]);

// Sets the history to the given value and navigates to the last item
// in the history.
const setHistory = async (history: Route[]) => {
  if (history.length === 0) {
    throw new error.Error({
      code: error.Code.EmptyHistory,
      message: "Cannot set empty history",
    });
  }
  const targetRoute = history.slice(-1)[0];

  const loadedRoute = await screen.withLock(() => loadRoute(targetRoute));
  activeRouteStore.set(loadedRoute);
  historyStore.set(history);
  window.history.replaceState(
    history,
    DOCUMENT_TITLE,
    routeToPath(targetRoute)
  );
};

export const push = async (newRoute: Route): Promise<void> => {
  const history = svelteStore.get(historyStore);
  // Limit history to a maximum of 10 steps. We shouldn't be doing more than
  // one subsequent pop() anyway.
  await setHistory([...history, newRoute].slice(-10));
};

export const pop = async (): Promise<void> => {
  const history = svelteStore.get(historyStore);
  if (history.length > 1) {
    await setHistory(history.slice(0, -1));
  }
};

export const activeRouteStore =
  svelteStore.writable<LoadedRoute>(INITIAL_ROUTE);

export const initialize = async (): Promise<void> => {
  let history: Route[];
  if (window.history.state === null) {
    history = [DEFAULT_ROUTE];
  } else {
    history = window.history.state;
  }
  await setHistory(history);
};
