// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Where the person is, in the address's hash, so any static host serves every
// page and the browser's back button works: "#/" is My documents and
// "#/d/<familyId>/<tab>" a document's page.

import { useEffect, useState } from "react";

export type Tab = "versions" | "sent" | "tree";
export const TABS: readonly Tab[] = ["versions", "sent", "tree"];

export type Route =
  | { readonly page: "library" }
  | { readonly page: "document"; readonly family: string; readonly tab: Tab };

const ID = /^[0-9a-f]{32}$/;

export function parse(hash: string): Route {
  const [, page, family, tab] = hash.replace(/^#/, "").split("/");
  if (page === "d" && family && ID.test(family)) {
    return {
      page: "document",
      family,
      tab: TABS.includes(tab as Tab) ? (tab as Tab) : "versions",
    };
  }
  return { page: "library" };
}

export function href(route: Route): string {
  return route.page === "library" ? "#/" : `#/d/${route.family}/${route.tab}`;
}

export function go(route: Route): void {
  window.location.hash = href(route);
}

/** The current route, following the address. */
export function useRoute(): Route {
  const [route, setRoute] = useState(() => parse(window.location.hash));
  useEffect(() => {
    const follow = () => {
      setRoute(parse(window.location.hash));
    };
    window.addEventListener("hashchange", follow);
    return () => {
      window.removeEventListener("hashchange", follow);
    };
  }, []);
  return route;
}
