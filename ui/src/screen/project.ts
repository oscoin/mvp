import { derived, Readable } from "svelte/store";

import * as error from "../error";
import * as project from "../project";
import * as remote from "../remote";
import * as source from "../source";

export enum CodeView {
  File = "FILE",
  Root = "ROOT",
}

interface Shared {
  peer: project.User;
  revision: source.Revision;
}

interface File extends Shared {
  kind: CodeView.File;
  file: source.Blob | error.Error;
  project: project.Project;
}

interface Root extends Shared {
  kind: CodeView.Root;
  lastCommit: source.LastCommit;
  project: project.Project;
  readme: source.Readme | null;
}

type Code = File | Root;

export const code: Readable<remote.Data<Code>> = derived(
  [project.project, project.selectedPeer, project.selectedRevision],
  ([currentProject, selectedPeer, selectedRevision], set) => {
    if (
      currentProject.status === remote.Status.NotAsked ||
      currentProject.status === remote.Status.Loading
    ) {
      set(currentProject);
    }

    if (!selectedPeer || !selectedRevision) {
      return;
    }

    if (currentProject.status === remote.Status.Success) {
      const { urn: projectUrn } = currentProject.data;
      let lastCommit: source.LastCommit;

      source
        .fetchObject(
          source.ObjectType.Tree,
          projectUrn,
          selectedPeer.peerId,
          "",
          selectedRevision
        )
        .then(tree => {
          lastCommit = tree.info.lastCommit;

          return source.readme(
            projectUrn,
            selectedPeer.peerId,
            selectedRevision,
            tree as source.Tree
          );
        })
        .then(readme => {
          set({
            status: remote.Status.Success,
            data: {
              kind: CodeView.Root,
              lastCommit,
              peer: selectedPeer,
              project: currentProject.data,
              readme: readme,
              revision: selectedRevision,
            },
          });
        });
    }
  },
  { status: remote.Status.NotAsked } as remote.Data<Code>
);
