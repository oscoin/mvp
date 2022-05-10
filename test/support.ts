// Copyright © 2022 The Radicle Upstream Contributors
//
// This file is part of radicle-upstream, distributed under the GPLv3
// with Radicle Linking Exception. For full terms see the included
// LICENSE file.

import * as Os from "node:os";
import * as Fs from "node:fs/promises";
import * as Path from "node:path";
import execa from "execa";
import waitOn from "wait-on";
import Semver from "semver";

import * as PeerRunner from "./support/peerRunner";
import * as Process from "./support/process";
import { retryOnError } from "ui/src/retryOnError";

// Assert that the docker container with the test git-server is
// running. If it is not running, throw an error that explains how to
// run it.
export async function assertGitServerRunning(): Promise<void> {
  const containerName = "upstream-git-server-test";
  const notRunningMessage =
    "The git-server test container is required for this test. You can run it with `./scripts/git-server-test.sh`";
  try {
    const result = await execa("docker", [
      "container",
      "inspect",
      containerName,
      "--format",
      "{{.State.Running}}",
    ]);
    if (result.stdout !== "true") {
      throw new Error(notRunningMessage);
    }
  } catch (err: unknown) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    if ((err as any).stderr === `Error: No such container: ${containerName}`) {
      throw new Error(notRunningMessage);
    } else {
      throw err;
    }
  }
}

// Assert that the `rad` CLI is installed and has the correct version.
export async function assertRadInstalled(): Promise<void> {
  const result = await execa("rad", ["--version"]);
  const versionConstraint = ">=0.4.0";
  const version = result.stdout.replace("rad ", "");
  if (!Semver.satisfies(version, versionConstraint)) {
    throw new Error(
      `rad version ${version} does not satisfy ${versionConstraint}`
    );
  }
}

// Returns a path to a directory where the test can store files.
//
// The directory is cleared before it is returned.
export async function prepareStateDir(
  testPath: string,
  testName: string
): Promise<string> {
  const stateDir = Path.resolve(`${testPath}--state`, testName);
  await Fs.rm(stateDir, { recursive: true, force: true });
  await Fs.mkdir(stateDir, { recursive: true });
  return stateDir;
}

export async function startSshAgent(): Promise<string> {
  // We’re not using the state directory because of the size limit on
  // the socket path.
  const dir = await Fs.mkdtemp(Path.join(Os.tmpdir(), "upstream-test"));
  const sshAuthSock = Path.join(dir, "ssh-agent.sock");
  Process.spawn("ssh-agent", ["-D", "-a", sshAuthSock], {
    stdio: "inherit",
  });
  await waitOn({ resources: [sshAuthSock], timeout: 5000 });
  return sshAuthSock;
}

// Call `fn` until it does not throw an error and return the result. Re-throws
// the error raised by `fn()` if it still fails after two seconds.
export function retry<T>(fn: () => Promise<T>): Promise<T> {
  return retryOnError(fn, () => true, 100, 20);
}

// Create a project using the rad CLI.
export async function createProject(
  proxy: PeerRunner.UpstreamPeer,
  name: string
): Promise<{ urn: string; checkoutPath: string }> {
  const checkoutPath = Path.join(proxy.checkoutPath, name);
  await proxy.spawn("git", ["init", checkoutPath, "--initial-branch", "main"]);
  await proxy.spawn(
    "git",
    ["commit", "--allow-empty", "--message", "initial commit"],
    {
      cwd: checkoutPath,
    }
  );
  await proxy.spawn(
    "rad",
    ["init", "--name", name, "--default-branch", "main", "--description", ""],
    {
      cwd: checkoutPath,
    }
  );

  const { stdout: urn } = await proxy.spawn("rad", ["inspect"], {
    cwd: checkoutPath,
  });

  await proxy.spawn(
    "git",
    ["config", "--add", "rad.seed", PeerRunner.SEED_URL],
    {
      cwd: checkoutPath,
    }
  );

  return { urn, checkoutPath };
}

// Create and publish a project using the rad CLI and return the project’s URN.
// Wait until the proxy registers the seed for the project.
export async function createAndPublishProject(
  proxy: PeerRunner.UpstreamPeer,
  name: string
): Promise<string> {
  const { urn, checkoutPath } = await createProject(proxy, name);

  await proxy.spawn("rad", ["push"], {
    cwd: checkoutPath,
  });

  await retry(async () => {
    const project = await proxy.proxyClient.project.get(urn);
    if (project.seed === null) {
      throw new Error("Proxy hasn't set the project seed yet.");
    }
  });

  return urn;
}
