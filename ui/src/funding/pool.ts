import { writable } from "svelte/store";
import { PoolFactory } from "radicle-contracts/build/contract-bindings/ethers/PoolFactory";
import { Pool as PoolContract } from "radicle-contracts/contract-bindings/ethers/Pool";
import * as validation from "../validation";

import { Wallet } from "../wallet";
import * as remote from "../remote";
import { BigNumberish } from "ethers";
import { intros } from "svelte/internal";

export const store = writable<Pool | null>(null);

export interface Pool {
  data: remote.Store<PoolData>;

  // Update the contribution amount per block. Returns once the
  // transaction has been included in the chain.
  updateAmountPerBlock(amountPerBlock: string): Promise<void>;

  // Update the list of receiver addresses. Returns once the
  // transaction has been included in the chain.
  updateReceiverAddresses(data: PoolData, addresses: string[]): Promise<void>;

  // Adds funds to the pool. Returns once the transaction has been
  // included in the chain.
  topUp(value: number): Promise<void>;
  // Collect funds the user has received up to now from givers and
  // transfer them to the users account.
  collect(): Promise<void>;
}

// The pool settings the user can change and save.
export interface PoolSettings {
  // The total amount to be disbursed to all receivers with each block.
  amountPerBlock: string;
  // The list of addresses that receive funds from the pool.
  receiverAddresses: string[];
}

// All the data representing a pool.
export interface PoolData {
  // The remaining balance of this pool.
  balance: number;
  // The total amount to be disbursed to all receivers with each block.
  amountPerBlock: string;
  // The list of addresses that receive funds from the pool.
  receiverAddresses: string[];
  // Funds that the user can collect from their givers.
  collectableFunds: number;
}

export function make(wallet: Wallet): Pool {
  const data = remote.createStore<PoolData>();
  const address = "0x0e22b57c7e69d1b62c9e4c88bb63b0357a905d1e";

  const poolContract: PoolContract = PoolFactory.connect(
    address,
    wallet.signer
  );

  loadPoolData();

  async function loadPoolData() {
    try {
      const balance = await poolContract.withdrawable();
      const collectableFunds = await poolContract.collectable();
      const amountPerBlock = await poolContract.getAmountPerBlock();
      const receivers = await poolContract.getAllReceivers();
      const receiverAddresses = receivers.map(r => r.receiver);

      data.success({
        // Handle potential overflow using BN.js
        balance: Number(balance),
        amountPerBlock: amountPerBlock.toString(),
        receiverAddresses,
        // Handle potential overflow using BN.js
        collectableFunds: Number(collectableFunds),
      });
    } catch (error) {
      data.error(error);
    }
  }

  async function updateAmountPerBlock(
    amountPerBlock: BigNumberish
  ): Promise<void> {
    await poolContract
      .setAmountPerBlock(amountPerBlock)
      .then(tx => tx.wait())
      .finally(loadPoolData);
  }

  async function updateReceiverAddresses(
    data: PoolData,
    addresses: string[]
  ): Promise<void> {
    // TODO(nuno): Read instance `data` instead of receiving as an argument.
    const newAddresses = addresses.filter(
      x => !data.receiverAddresses.includes(x)
    );
    const txs = newAddresses.map(address =>
      poolContract.setReceiver(address, 1).then(tx => tx.wait())
    );

    // TODO check transaction status
    await Promise.all(txs).finally(loadPoolData);
  }

  async function topUp(value: number): Promise<void> {
    const tx = await poolContract.topUp({
      gasLimit: 200 * 1000,
      value,
    });
    const receipt = await tx.wait();
    if (receipt.status === 0) {
      throw new Error(`Transaction reverted: ${receipt.transactionHash}`);
    }
    loadPoolData();
  }

  async function collect(): Promise<void> {
    const tx = await poolContract.collect();
    const receipt = await tx.wait();
    if (receipt.status === 0) {
      throw new Error(`Transaction reverted: ${receipt.transactionHash}`);
    }
    loadPoolData();
  }

  return {
    data,
    updateAmountPerBlock,
    updateReceiverAddresses,
    topUp,
    collect,
  };
}

/**
 * Stores
 */
export const membersStore = writable("");
export const amountStore = writable("");

/**
 *
 * Validations
 *
 */

// Patterns
const COMMA_SEPARATED_LIST = /(^[-\w\s]+(?:,[-\w\s]*)*$)?/;

const contraints = {
  // The contraints for a valid input list of members.
  members: {
    format: {
      pattern: COMMA_SEPARATED_LIST,
      message: `Should be a comma-separated list of addresses`,
    },
  },

  // The contraints for a valid amount input.
  amount: {
    presence: {
      message: "The amount is required",
      allowEmpty: false,
    },
    numericality: {
      strict: true,
      greaterThan: 0,
    },
  },
};

export const membersValidationStore: validation.ValidationStore = validation.createValidationStore(
  contraints.members
);

export const amountValidationStore: validation.ValidationStore = validation.createValidationStore(
  contraints.amount
);

/* Temporary sketch code */

enum TxStatus {
  // The transaction is pending user approval on their waLlet app.
  PendingApproval,
  // The transaction as been approved and is awaiting to be included in a block.
  AwaitingInclusion,
  // The transaction as been included in the block. End of its life cycle.
  Included,
  // The transaction as been rejected.
  Rejected,
}

enum PoolTxKind {
  TopUp,
  CollectFunds,
  UpdateMonthlyContribution,
  UpdateBeneficiaries,
}

interface TopUp {
  kind: PoolTxKind.TopUp;
  amount: string;
}

interface CollectFunds {
  kind: PoolTxKind.CollectFunds;
  amount: string;
}

interface UpdateMonthlyContribution {
  kind: PoolTxKind.UpdateMonthlyContribution;
  // The value the monthly contribution is being set to.
  amount: string;
}

interface UpdateMonthlyContribution {
  kind: PoolTxKind.UpdateMonthlyContribution;
  // The value the monthly contribution is being set to.
  amount: string;
}

interface UpdateBeneficiaries {
  kind: PoolTxKind.UpdateMonthlyContribution;
}

type PoolTx =
  | TopUp
  | CollectFunds
  | UpdateMonthlyContribution
  | UpdateBeneficiaries;

interface Tx {
  // The hash of the transaction that uniquely identifies it.
  hash: string;

  // The status of the transaction
  status: TxStatus;

  // The underlying transaction.
  inner: PoolTx;
}

const transactions: Tx[] = [];

function addTx(tx: Tx) {
  transactions.push(tx);
}

function updateTxStatus(hash: string, status: TxStatus) {
  const tx = transactions.find(tx => tx.hash === hash);
  if (tx) {
    tx.status = status;
  }
}

// Cap the amount of managed transactions
function cap() {
  transactions.length = Math.min(transactions.length, 5);
}

function updateAll() {
  transactions.forEach(tx => {
    const newStatus = lookupStatus(tx.hash);
    if (newStatus) tx.status = newStatus;
  });
}

// TODO(nuno): Lookup the actual status of a transaction with the given hash.
function lookupStatus(_hash: string): TxStatus | undefined {
  function randomInt(max: number) {
    return Math.floor(Math.random() * Math.floor(max));
  }

  const statuses = [
    TxStatus.PendingApproval,
    TxStatus.AwaitingInclusion,
    TxStatus.Included,
    TxStatus.Rejected,
  ];
  return statuses[randomInt(statuses.length)];
}
