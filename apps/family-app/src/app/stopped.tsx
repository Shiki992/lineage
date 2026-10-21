// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The last resort: when a page throws while it draws, My documents says so in
// plain words instead of going blank, and offers the details for a bug
// report. They are shown before they are copied, and nothing is sent anywhere
// (decision 0031).

import { Component, useState } from "react";
import type { ErrorInfo, ReactNode } from "react";
import { Button } from "./parts";

const BUILT: unknown = import.meta.env["VITE_LINEAGE_VERSION"];
/** "1.4.0" in a release build, otherwise "development". */
export const VERSION = typeof BUILT === "string" && BUILT !== "" ? BUILT : "development";

interface State {
  readonly error: Error | null;
}

export class Stopped extends Component<{ readonly children: ReactNode }, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: unknown): State {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error(error, info.componentStack);
  }

  override render(): ReactNode {
    return this.state.error ? <StoppedScreen error={this.state.error} /> : this.props.children;
  }
}

function StoppedScreen({ error }: { error: Error }) {
  const [copy, setCopy] = useState<"idle" | "copied" | "manual">("idle");
  const details = [
    `Lineage ${VERSION}, ${navigator.userAgent}`,
    `${error.name}: ${error.message}`,
    ...(error.stack ?? "").split("\n").slice(1, 6),
  ].join("\n");
  return (
    <main className="content stack" role="alert">
      <h1>Lineage stopped working</h1>
      <p>Something went wrong while this page was drawn. Reload the page to carry on.</p>
      <div>
        <Button
          primary
          onClick={() => {
            window.location.reload();
          }}
        >
          Reload the page
        </Button>
      </div>
      <details>
        <summary>Details for a bug report</summary>
        <pre className="code-scroll small">{details}</pre>
        {copy === "idle" && (
          <Button
            onClick={() => {
              navigator.clipboard.writeText(details).then(
                () => {
                  setCopy("copied");
                },
                () => {
                  setCopy("manual");
                },
              );
            }}
          >
            Copy details
          </Button>
        )}
        {copy === "copied" && (
          <p className="small secondary" role="status">
            Copied. Nothing was sent anywhere.
          </p>
        )}
        {copy === "manual" && (
          <p className="small secondary" role="status">
            The clipboard isn&apos;t available here. Select the details above and copy them.
          </p>
        )}
      </details>
      <p className="small secondary">Lineage {VERSION}</p>
    </main>
  );
}
