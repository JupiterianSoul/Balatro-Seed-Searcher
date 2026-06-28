// Preset filter library — famous/useful Balatro seed configurations

import type { Filter } from './types';

export type Preset = {
  id: string;
  label: string;
  description: string;
  filter: Filter;
};

export const PRESETS: Preset[] = [
  {
    id: 'blueprint-brainstorm-a1',
    label: 'Blueprint + Brainstorm ante 1',
    description: 'Find seeds where Blueprint and Brainstorm both appear in the ante 1 shop.',
    filter: {
      clauses: [
        { kind: 'ante_shop_has_joker', ante: 1, slot: 0, joker: 'Blueprint' },
        { kind: 'ante_shop_has_joker', ante: 1, slot: 1, joker: 'Brainstorm' },
      ],
      partial: true,
      min_score: 1,
    },
  },
  {
    id: 'triboulet-soul',
    label: 'Triboulet via Soul pack',
    description: 'Find seeds where a Spectral pack containing The Soul appears, yielding Triboulet.',
    filter: {
      clauses: [
        { kind: 'ante_pack_contains', ante: 1, pack_index: 0, card: 'The Soul' },
      ],
      partial: true,
      min_score: 1,
    },
  },
  {
    id: 'negative-joker-a1',
    label: 'Negative Joker ante 1 shop',
    description: 'Find seeds with a Negative-edition joker appearing in the ante 1 shop.',
    filter: {
      clauses: [
        { kind: 'ante_shop_has_joker', ante: 1, slot: 0, joker: 'Joker', edition: 'Negative' },
      ],
      partial: true,
      min_score: 1,
    },
  },
  {
    id: 'three-rare-a1-a3',
    label: 'Three rare jokers ante 1-3',
    description: 'Seeds with a rare joker in the shop slots of antes 1, 2, and 3.',
    filter: {
      clauses: [
        { kind: 'ante_shop_has_joker', ante: 1, slot: 0, joker: 'Triboulet' },
        { kind: 'ante_shop_has_joker', ante: 2, slot: 0, joker: 'Canio' },
        { kind: 'ante_shop_has_joker', ante: 3, slot: 0, joker: 'Yorick' },
      ],
      partial: true,
      min_score: 2,
    },
  },
  {
    id: 'all-finisher',
    label: 'All-finisher run',
    description: 'Amber Acorn + Verdant Leaf + Violet Vessel + Crimson Heart + Cerulean Bell tags in a single run.',
    filter: {
      clauses: [
        { kind: 'ante_tag_is', ante: 1, position: 0, tag: 'Amber Acorn' },
        { kind: 'ante_tag_is', ante: 2, position: 0, tag: 'Verdant Leaf' },
        { kind: 'ante_tag_is', ante: 3, position: 0, tag: 'Violet Vessel' },
        { kind: 'ante_tag_is', ante: 4, position: 0, tag: 'Crimson Heart' },
        { kind: 'ante_tag_is', ante: 5, position: 0, tag: 'Cerulean Bell' },
      ],
      partial: true,
      min_score: 3,
    },
  },
];
