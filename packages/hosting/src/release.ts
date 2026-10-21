// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// What a release build is told, and the check that it was told all of it
// (decision 0031). The deploy workflow sets these from its GitHub Environment;
// nothing here is committed. A build without LINEAGE_RELEASE=1 is not a
// release: it may leave them out, and is never deployed.

export type Channel = "staging" | "production";

export interface Release {
  readonly channel: Channel;
  /** `1.4.0`, from the tag. */
  readonly version: string;
  /** The workflow's run number, so every manifest is newer than the last. */
  readonly build: number;
  /** Where the task pane is served, `https://host` with no path. */
  readonly addinOrigin: string;
  /** Where My documents is served. */
  readonly appOrigin: string;
  readonly clientId: string;
}

const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const VERSION = /^(0|[1-9][0-9]{0,4})\.(0|[1-9][0-9]{0,4})\.(0|[1-9][0-9]{0,4})$/;

function origin(name: string, value: string | undefined, problems: string[]): string {
  try {
    const url = new URL(value ?? "");
    if (
      url.protocol === "https:" &&
      url.origin === value &&
      !["localhost", "127.0.0.1", "[::1]"].includes(url.hostname)
    ) {
      return value;
    }
  } catch {
    // Reported below.
  }
  problems.push(`${name} must be an https origin with no path, such as https://addin.example.org`);
  return "";
}

/**
 * The release a build is for, or none when LINEAGE_RELEASE isn't "1".
 *
 * @throws Error listing every setting that is missing or malformed.
 */
export function releaseFrom(
  env: Readonly<Record<string, string | undefined>>,
): Release | undefined {
  if (env["LINEAGE_RELEASE"] !== "1") {
    return undefined;
  }
  const problems: string[] = [];
  const channel = env["LINEAGE_CHANNEL"];
  if (channel !== "staging" && channel !== "production") {
    problems.push("LINEAGE_CHANNEL must be staging or production");
  }
  const version = env["VITE_LINEAGE_VERSION"] ?? "";
  if (!VERSION.test(version)) {
    problems.push("VITE_LINEAGE_VERSION must be three numbers, such as 1.4.0");
  }
  const build = Number(env["LINEAGE_BUILD"] ?? "0");
  if (!Number.isInteger(build) || build < 0 || build > 65535) {
    problems.push("LINEAGE_BUILD must be a whole number from 0 to 65535");
  }
  const addinOrigin = origin("LINEAGE_ADDIN_ORIGIN", env["LINEAGE_ADDIN_ORIGIN"], problems);
  const appOrigin = origin("LINEAGE_APP_ORIGIN", env["LINEAGE_APP_ORIGIN"], problems);
  if (addinOrigin && addinOrigin === appOrigin) {
    problems.push("the task pane and My documents need origins of their own");
  }
  const clientId = env["VITE_LINEAGE_CLIENT_ID"] ?? "";
  if (!GUID.test(clientId)) {
    problems.push("VITE_LINEAGE_CLIENT_ID must be the app registration's client ID");
  }
  if (appOrigin && env["VITE_LINEAGE_APP_URL"] !== `${appOrigin}/`) {
    problems.push("VITE_LINEAGE_APP_URL must be LINEAGE_APP_ORIGIN followed by /");
  }
  if (problems.length > 0) {
    throw new Error(`This release build isn't set up:\n- ${problems.join("\n- ")}`);
  }
  return { channel: channel as Channel, version, build, addinOrigin, appOrigin, clientId };
}
