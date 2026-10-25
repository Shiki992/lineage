// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// "Share a link": a sent copy shared as a OneDrive link instead of attached
// (decision 0032). The copy shared is exactly the one that left. With "Can
// edit", what they change comes back to review like any file that came back;
// the record of what left never changes.

import type { LinkAccess } from "@lineage/shell";
import { useId, useState } from "react";
import { Back, Button } from "./parts";

export interface MadeLink {
  readonly url: string;
  /** "Anyone with the link can open it". */
  readonly who: string;
}

export function ShareLink({
  recipient,
  storage,
  made,
  busy,
  onShare,
  onBack,
}: {
  recipient: string;
  /** "OneDrive", or the development folder. */
  storage: string;
  /** The link, once it's made. */
  made: (MadeLink & { readonly access: LinkAccess }) | null;
  busy: boolean;
  onShare: (access: LinkAccess) => void;
  onBack: () => void;
}) {
  const [access, setAccess] = useState<LinkAccess>("view");
  const id = useId();
  if (made) {
    return (
      <div className="stack">
        <Back onClick={onBack} />
        <h1>Link for {recipient}</h1>
        <p>{made.who}.</p>
        <CopyLink url={made.url} />
        {made.access === "edit" ? (
          <p className="secondary small">
            When they change it, What I&apos;ve sent says so, and you review their changes one at a
            time, like a copy that came back.
          </p>
        ) : (
          <p className="secondary small">They can open it, but not change it.</p>
        )}
        <p className="secondary small">
          Stop sharing it any time from What I&apos;ve sent. What left is kept exactly as it was.
        </p>
      </div>
    );
  }
  return (
    <div className="stack">
      <Back onClick={onBack} />
      <h1>Share a link</h1>
      <p>
        Instead of attaching the copy for {recipient}, send them a link to it in {storage}.
      </p>
      <fieldset className="choices">
        <legend className="small" style={{ fontWeight: 600 }}>
          They can
        </legend>
        <label className="choice card">
          <input
            type="radio"
            name={`${id}-access`}
            checked={access === "view"}
            onChange={() => {
              setAccess("view");
            }}
          />
          <span>
            <strong>View only</strong>
            <span className="block secondary small">Open and read it.</span>
          </span>
        </label>
        <label className="choice card">
          <input
            type="radio"
            name={`${id}-access`}
            checked={access === "edit"}
            onChange={() => {
              setAccess("edit");
            }}
          />
          <span>
            <strong>Edit</strong>
            <span className="block secondary small">
              Change it. Their changes come back to you to keep or skip.
            </span>
          </span>
        </label>
      </fieldset>
      {storage === "OneDrive" && (
        <p className="secondary small">
          Microsoft may ask you once to let Lineage share files. Its screen says “all your files”;
          Lineage only shares the copies you choose.
        </p>
      )}
      <Button
        primary
        wide
        disabled={busy}
        onClick={() => {
          onShare(access);
        }}
      >
        Make the link
      </Button>
    </div>
  );
}

/** The link, selectable, with a button to copy it where the clipboard allows. */
export function CopyLink({ url }: { url: string }) {
  const [state, setState] = useState<"idle" | "copied" | "manual">("idle");
  const id = useId();
  return (
    <div className="stack" style={{ gap: 6 }}>
      <label htmlFor={id} className="small" style={{ fontWeight: 600 }}>
        Link
      </label>
      <input
        id={id}
        className="input mono"
        readOnly
        value={url}
        onFocus={(e) => {
          e.currentTarget.select();
        }}
      />
      <div className="row">
        <Button
          onClick={() => {
            navigator.clipboard.writeText(url).then(
              () => {
                setState("copied");
              },
              () => {
                setState("manual");
              },
            );
          }}
        >
          Copy link
        </Button>
        {state === "copied" && (
          <span className="small secondary" role="status">
            Copied.
          </span>
        )}
      </div>
      {state === "manual" && (
        <p className="small secondary" role="status">
          The clipboard isn&apos;t available here. Select the link above and copy it.
        </p>
      )}
    </div>
  );
}
