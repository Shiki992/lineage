// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// My documents: sign in with the account the person already has (there are no
// Lineage accounts), then the library and each document's page. Everything is
// read from the person's own storage, in this browser.

import type { Engine } from "@lineage/shell";
import { Library, userMessage } from "@lineage/shell";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { AccountProvider, AccountStatus } from "../account/account";
import { DocumentScreen } from "./document";
import type { Documents } from "./documents";
import { LibraryScreen } from "./library";
import { Alert, Button, Loading } from "./parts";
import { go, useRoute } from "./route";

export interface AppProps {
  readonly account: AccountProvider;
  readonly engine: Engine;
  /** For tests: the history to show instead of a Library over the account's storage. */
  readonly documents?: (account: AccountProvider) => Documents;
}

export function App({ account, engine, documents }: AppProps) {
  const [status, setStatus] = useState<AccountStatus | undefined>();
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | undefined>();

  useEffect(() => {
    let live = true;
    void account.status().then((s) => {
      if (live) {
        setStatus(s);
      }
    });
    return () => {
      live = false;
    };
  }, [account]);

  const signedIn = status?.kind === "signedIn";
  const history = useMemo(
    () =>
      signedIn
        ? (documents?.(account) ?? new Library({ engine, storage: account.storage() }))
        : undefined,
    [signedIn, account, engine, documents],
  );

  const signIn = useCallback(async () => {
    setBusy(true);
    setProblem(undefined);
    try {
      await account.signIn();
      setStatus(await account.status());
    } catch (error) {
      setProblem(userMessage(error));
    } finally {
      setBusy(false);
    }
  }, [account]);

  const signOut = useCallback(async () => {
    setBusy(true);
    try {
      await account.signOut();
      go({ page: "library" });
      setStatus({ kind: "signedOut" });
    } catch (error) {
      setProblem(userMessage(error));
    } finally {
      setBusy(false);
    }
  }, [account]);

  return (
    <div className="app">
      <header className="masthead">
        <a className="brand" href="#/">
          Lineage
        </a>
        {status?.kind === "signedIn" && (
          <div className="row">
            <span className="secondary small">
              {status.person.name}
              {status.person.username !== status.person.name && (
                <span className="visually-hidden"> ({status.person.username})</span>
              )}
            </span>
            <Button disabled={busy} onClick={() => void signOut()}>
              Sign out
            </Button>
          </div>
        )}
      </header>
      <main className="content">
        {status === undefined && <Loading what="Starting…" />}
        {status !== undefined && status.kind !== "signedIn" && (
          <SignIn
            account={account}
            busy={busy}
            message={problem ?? (status.kind === "failed" ? status.message : undefined)}
            onSignIn={() => void signIn()}
          />
        )}
        {history && <Pages documents={history} where={account.where} />}
      </main>
    </div>
  );
}

function Pages({ documents, where }: { documents: Documents; where: string }) {
  const route = useRoute();
  return route.page === "library" ? (
    <LibraryScreen documents={documents} where={where} />
  ) : (
    <DocumentScreen documents={documents} family={route.family} tab={route.tab} />
  );
}

function SignIn({
  account,
  busy,
  message,
  onSignIn,
}: {
  account: AccountProvider;
  busy: boolean;
  message: string | undefined;
  onSignIn: () => void;
}) {
  return (
    <section className="panel stack sign-in" aria-labelledby="sign-in-title">
      <h1 id="sign-in-title">My documents</h1>
      <p>
        See every document you track with Lineage: all its versions, every copy you sent, and the
        family tree.
      </p>
      <p className="secondary">
        There&apos;s no Lineage account. Sign in with the account you use for Lineage in Word or
        PowerPoint, and your history is read from {account.where}, in this browser only.
      </p>
      {message && <Alert>{message}</Alert>}
      <div>
        <Button primary disabled={busy} onClick={onSignIn}>
          {account.signInLabel}
        </Button>
      </div>
    </section>
  );
}
