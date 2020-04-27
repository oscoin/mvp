import ApolloClient from "apollo-boost";
import { gql } from "apollo-boost";

const controlClient = new ApolloClient({
  uri: "http://localhost:8080/control"
});

Cypress.Commands.add("nukeCocoState", () => {
  console.log("Nuking CoCo state");
  controlClient.mutate({
    mutation: gql`
      mutation {
        nukeCocoState
      }
    `
  });
});

Cypress.Commands.add("nukeRegistryState", () => {
  console.log("Nuking Registry state");
  controlClient.mutate({
    mutation: gql`
      mutation {
        nukeRegistryState
      }
    `
  });
});

Cypress.Commands.add("nukeSessionState", () => {
  console.log("Nuking Session state");
  controlClient.mutate({
    mutation: gql`
      mutation {
        nukeSessionState
      }
    `
  });
});

Cypress.Commands.add("nukeAllState", () => {
  console.log("Nuking CoCo and Registry state");
  controlClient.mutate({
    mutation: gql`
      mutation {
        nukeCocoState
        nukeRegistryState
        nukeSessionState
      }
    `
  });
});

Cypress.Commands.add(
  "createProjectWithFixture",
  (
    name = "Monadic",
    description = "Monadic is currently supporting radicle.",
    defaultBranch = "master"
  ) => {
    controlClient.mutate({
      variables: {
        name: name,
        description: description,
        defaultBranch: defaultBranch
      },
      mutation: gql`
        mutation CreateProjectWithFixture(
          $name: String!
          $description: String!
          $defaultBranch: String!
        ) {
          createProjectWithFixture(
            metadata: {
              name: $name
              description: $description
              defaultBranch: $defaultBranch
            }
          ) {
            id
          }
        }
      `
    });
  }
);

Cypress.Commands.add("registerUser", (handle = "nope", id = "123abcd.git") => {
  controlClient.mutate({
    variables: {
      handle: handle,
      id: id
    },
    mutation: gql`
      mutation RegisterUser($handle: ID!, $id: ID!) {
        registerUser(handle: $handle, id: $id) {
          messages {
            ... on UserRegistrationMessage {
              handle
              id
            }
          }
        }
      }
    `
  });
});

Cypress.Commands.add(
  "createIdentity",
  (
    handle = "secretariat",
    displayName = "Christopher Chenery",
    avatarUrl = null
  ) =>
    fetch("http://localhost:8080/v1/identities", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        handle,
        displayName,
        avatarUrl
      })
    })
);
