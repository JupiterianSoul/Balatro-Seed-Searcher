// Shared type definitions for the Balatro Seed Searcher

export type ClauseAnteShopHasJoker = {
  kind: 'ante_shop_has_joker';
  ante: number;
  slot: number;
  joker: string;
  edition?: string;
};

export type ClauseAnteTagIs = {
  kind: 'ante_tag_is';
  ante: number;
  position: number;
  tag: string;
};

export type ClauseAnteBossIs = {
  kind: 'ante_boss_is';
  ante: number;
  boss: string;
};

export type ClauseVoucherIs = {
  kind: 'voucher_is';
  ante: number;
  voucher: string;
};

export type ClauseAntePackContains = {
  kind: 'ante_pack_contains';
  ante: number;
  pack_index: number;
  card: string;
};

export type FilterClause =
  | ClauseAnteShopHasJoker
  | ClauseAnteTagIs
  | ClauseAnteBossIs
  | ClauseVoucherIs
  | ClauseAntePackContains;

export type Filter = {
  clauses: FilterClause[];
  partial: boolean;
  min_score?: number;
};

export type SearchConfig = {
  seedLen: number;
  deckIdx: number;
  stakeIdx: number;
  topN: number;
};

export type MatchRecord = {
  rank: bigint;
  score: number;
  seed: string;
};

// Worker message types

export type WorkerInboundScan = {
  type: 'scan';
  filter: Filter;
  startRank: bigint;
  count: bigint;
  seedLen: number;
  deckIdx: number;
  stakeIdx: number;
  partial: boolean;
  minScore: number;
  workerId: number;
};

export type WorkerInboundStop = {
  type: 'stop';
  workerId: number;
};

export type WorkerInbound = WorkerInboundScan | WorkerInboundStop;

export type WorkerOutboundMatches = {
  type: 'matches';
  workerId: number;
  matches: MatchRecord[];
  scanned: bigint;
};

export type WorkerOutboundProgress = {
  type: 'progress';
  workerId: number;
  scanned: bigint;
  elapsedMs: number;
};

export type WorkerOutboundDone = {
  type: 'done';
  workerId: number;
  totalScanned: bigint;
};

export type WorkerOutboundError = {
  type: 'error';
  workerId: number;
  message: string;
};

export type WorkerOutbound =
  | WorkerOutboundMatches
  | WorkerOutboundProgress
  | WorkerOutboundDone
  | WorkerOutboundError;

// Orchestrator event payloads

export type OrchestratorProgressEvent = {
  seedsPerSec: number;
  totalScanned: bigint;
  matchCount: number;
  workerCount: number;
  elapsedMs: number;
};

export type OrchestratorMatchEvent = {
  matches: MatchRecord[];
};

export type OrchestratorDoneEvent = {
  totalScanned: bigint;
  matchCount: number;
  elapsedMs: number;
};

// Deck and stake names

export const DECK_NAMES = [
  'Red Deck',
  'Blue Deck',
  'Yellow Deck',
  'Green Deck',
  'Black Deck',
  'Magic Deck',
  'Nebula Deck',
  'Ghost Deck',
  'Abandoned Deck',
  'Checkered Deck',
  'Zodiac Deck',
  'Painted Deck',
  'Anaglyph Deck',
  'Plasma Deck',
  'Erratic Deck',
] as const;

export const STAKE_NAMES = [
  'White Stake',
  'Red Stake',
  'Green Stake',
  'Black Stake',
  'Blue Stake',
  'Purple Stake',
  'Orange Stake',
  'Gold Stake',
] as const;
