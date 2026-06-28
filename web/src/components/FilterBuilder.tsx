import { useCallback } from 'react';
import type {
  Filter,
  FilterClause,
  ClauseAnteShopHasJoker,
  ClauseAnteTagIs,
  ClauseAnteBossIs,
  ClauseVoucherIs,
  ClauseAntePackContains,
} from '../types';
import { PRESETS } from '../presets';

// ─── Game data lists ─────────────────────────────────────────────────────────

const JOKERS = [
  'Joker', 'Greedy Joker', 'Lusty Joker', 'Wrathful Joker', 'Gluttonous Joker',
  'Jolly Joker', 'Zany Joker', 'Mad Joker', 'Crazy Joker', 'Droll Joker',
  'Sly Joker', 'Wily Joker', 'Clever Joker', 'Devious Joker', 'Crafty Joker',
  'Half Joker', 'Joker Stencil', 'Four Fingers', 'Mime', 'Credit Card',
  'Ceremonial Dagger', 'Banner', 'Mystic Summit', 'Marble Joker', 'Loyalty Card',
  'Abstract Joker', 'Delayed Gratification', 'Hack', 'Pareidolia', 'Gros Michel',
  'Even Steven', 'Odd Todd', 'Scholar', 'Business Card', 'Supernova',
  'Ride the Bus', 'Space Joker', 'Egg', 'Burglar', 'Blackboard',
  'Runner', 'Ice Cream', 'DNA', 'Splash', 'Blue Joker',
  'Sixth Sense', 'Constellation', 'Hiker', 'Card Sharp', 'Red Card',
  'Madness', 'Square Joker', 'Séance', 'Riff-Raff', 'Vampire',
  'Shortcut', 'Hologram', 'Vagabond', 'Baron', 'Cloud 9',
  'Rocket', 'Obelisk', 'Midas Mask', 'Luchador', 'Photograph',
  'Gift Card', 'Turtle Bean', 'Erosion', 'Reserved Parking', 'Mail-In Rebate',
  'To the Moon', 'Hallucination', 'Fortune Teller', 'Juggler', 'Drunkard',
  'Stone Joker', 'Golden Joker', 'Lucky Cat', 'Bull', 'Diet Cola',
  'Trading Card', 'Flash Card', 'Popcorn', 'Ramen', 'Walkie Talkie',
  'Seltzer', 'Castle', 'Smiley Face', 'Campfire', 'Golden Ticket',
  'Mr. Bones', 'Acrobat', 'Sock and Buskin', 'Swashbuckler', 'Troubadour',
  'Certificate', 'Smeared Joker', 'Throwback', 'Hanging Chad', 'Rough Gem',
  'Bloodstone', 'Arrowhead', 'Onyx Agate', 'Glass Joker', 'Showman',
  'Flower Pot', 'Blueprint', 'Wee Joker', 'Merry Andy', 'Oops! All 6s',
  'The Idol', 'Seeing Double', 'Matador', 'Hit the Road', 'The Duo',
  'The Trio', 'The Family', 'The Order', 'The Tribe', 'Stuntman',
  'Invisible Joker', 'Brainstorm', 'Satellite', 'Shoot the Moon', 'Driver\'s License',
  'Cartomancer', 'Astronomer', 'Burnt Joker', 'Bootstraps',
  'Canio', 'Triboulet', 'Yorick', 'Chicot', 'Perkeo',
];

const EDITIONS = ['', 'Foil', 'Holographic', 'Polychrome', 'Negative'];

const TAGS = [
  'Uncommon Tag', 'Rare Tag', 'Negative Tag', 'Foil Tag', 'Holographic Tag',
  'Polychrome Tag', 'Investment Tag', 'Voucher Tag', 'Boss Tag', 'Standard Tag',
  'Charm Tag', 'Meteor Tag', 'Buffoon Tag', 'Handy Tag', 'Garbage Tag',
  'Ethereal Tag', 'Coupon Tag', 'Double Tag', 'Juggle Tag', 'D6 Tag',
  'Top-up Tag', 'Speed Tag', 'Orbital Tag', 'Economy Tag',
  'Amber Acorn', 'Verdant Leaf', 'Violet Vessel', 'Crimson Heart', 'Cerulean Bell',
];

const BOSSES = [
  'The Hook', 'The Ox', 'The House', 'The Wall', 'The Wheel',
  'The Arm', 'The Club', 'The Fish', 'The Psychic', 'The Goad',
  'The Water', 'The Window', 'The Manacle', 'The Eye', 'The Mouth',
  'The Plant', 'The Serpent', 'The Pillar', 'The Needle', 'The Head',
  'The Tooth', 'The Flint', 'The Mark',
  'Amber Acorn', 'Verdant Leaf', 'Violet Vessel', 'Crimson Heart', 'Cerulean Bell',
];

const VOUCHERS = [
  'Overstock', 'Clearance Sale', 'Liquidation', 'Hone', 'Reroll Surplus',
  'Crystal Ball', 'Telescope', 'Magic Trick', 'Hieroglyph', 'Directors Cut',
  'Paint Brush', 'Nails', 'Omen Globe', 'Tarot Merchant', 'Planet Merchant',
  'Seed Money', 'Money Tree', 'Blank', 'Antimatter', 'Overstock Plus',
  'Reroll Glut', 'Illusion', 'Ectoplasm', 'Petroglyph', 'Directors Cut',
  'Retcon', 'Palette',
];

const CARDS = [
  // Tarot
  'The Fool', 'The Magician', 'The High Priestess', 'The Empress', 'The Emperor',
  'The Hierophant', 'The Lovers', 'The Chariot', 'Strength', 'The Hermit',
  'Wheel of Fortune', 'Justice', 'The Hanged Man', 'Death', 'Temperance',
  'The Devil', 'The Tower', 'The Star', 'The Moon', 'The Sun',
  'Judgement', 'The World',
  // Spectral
  'Familiar', 'Grim', 'Incantation', 'Talisman', 'Aura', 'Wraith',
  'Sigil', 'Ouija', 'Ectoplasm', 'Immolate', 'Ankh', 'Deja Vu',
  'Hex', 'Trance', 'Medium', 'Cryptid', 'The Soul', 'Black Hole',
];

// ─── Clause defaults ─────────────────────────────────────────────────────────

function defaultClause(kind: FilterClause['kind']): FilterClause {
  switch (kind) {
    case 'ante_shop_has_joker':
      return { kind, ante: 1, slot: 0, joker: JOKERS[0] };
    case 'ante_tag_is':
      return { kind, ante: 1, position: 0, tag: TAGS[0] };
    case 'ante_boss_is':
      return { kind, ante: 1, boss: BOSSES[0] };
    case 'voucher_is':
      return { kind, ante: 1, voucher: VOUCHERS[0] };
    case 'ante_pack_contains':
      return { kind, ante: 1, pack_index: 0, card: CARDS[0] };
  }
}

// ─── Sub-editors ─────────────────────────────────────────────────────────────

type ClauseEditorProps<T extends FilterClause> = {
  clause: T;
  onChange: (updated: T) => void;
};

function AnteSelect({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  return (
    <select
      className="input-select"
      value={value}
      onChange={e => onChange(Number(e.target.value))}
      aria-label="Ante"
    >
      {[1, 2, 3, 4, 5, 6, 7, 8].map(a => (
        <option key={a} value={a}>Ante {a}</option>
      ))}
    </select>
  );
}

function ShopHasJokerEditor({ clause, onChange }: ClauseEditorProps<ClauseAnteShopHasJoker>) {
  return (
    <>
      <AnteSelect value={clause.ante} onChange={v => onChange({ ...clause, ante: v })} />
      <label className="clause-label">Slot</label>
      <select
        className="input-select"
        value={clause.slot}
        onChange={e => onChange({ ...clause, slot: Number(e.target.value) })}
        aria-label="Slot"
      >
        {[0, 1, 2, 3, 4].map(s => (
          <option key={s} value={s}>Slot {s + 1}</option>
        ))}
      </select>
      <label className="clause-label">Joker</label>
      <select
        className="input-select input-select--wide"
        value={clause.joker}
        onChange={e => onChange({ ...clause, joker: e.target.value })}
        aria-label="Joker"
      >
        {JOKERS.map(j => <option key={j} value={j}>{j}</option>)}
      </select>
      <label className="clause-label">Edition</label>
      <select
        className="input-select"
        value={clause.edition ?? ''}
        onChange={e => onChange({ ...clause, edition: e.target.value || undefined })}
        aria-label="Edition"
      >
        {EDITIONS.map(ed => <option key={ed} value={ed}>{ed || 'Any'}</option>)}
      </select>
    </>
  );
}

function TagIsEditor({ clause, onChange }: ClauseEditorProps<ClauseAnteTagIs>) {
  return (
    <>
      <AnteSelect value={clause.ante} onChange={v => onChange({ ...clause, ante: v })} />
      <label className="clause-label">Position</label>
      <select
        className="input-select"
        value={clause.position}
        onChange={e => onChange({ ...clause, position: Number(e.target.value) })}
        aria-label="Position"
      >
        {[0, 1].map(p => <option key={p} value={p}>Tag {p + 1}</option>)}
      </select>
      <label className="clause-label">Tag</label>
      <select
        className="input-select input-select--wide"
        value={clause.tag}
        onChange={e => onChange({ ...clause, tag: e.target.value })}
        aria-label="Tag"
      >
        {TAGS.map(t => <option key={t} value={t}>{t}</option>)}
      </select>
    </>
  );
}

function BossIsEditor({ clause, onChange }: ClauseEditorProps<ClauseAnteBossIs>) {
  return (
    <>
      <AnteSelect value={clause.ante} onChange={v => onChange({ ...clause, ante: v })} />
      <label className="clause-label">Boss</label>
      <select
        className="input-select input-select--wide"
        value={clause.boss}
        onChange={e => onChange({ ...clause, boss: e.target.value })}
        aria-label="Boss"
      >
        {BOSSES.map(b => <option key={b} value={b}>{b}</option>)}
      </select>
    </>
  );
}

function VoucherIsEditor({ clause, onChange }: ClauseEditorProps<ClauseVoucherIs>) {
  return (
    <>
      <AnteSelect value={clause.ante} onChange={v => onChange({ ...clause, ante: v })} />
      <label className="clause-label">Voucher</label>
      <select
        className="input-select input-select--wide"
        value={clause.voucher}
        onChange={e => onChange({ ...clause, voucher: e.target.value })}
        aria-label="Voucher"
      >
        {VOUCHERS.map(v => <option key={v} value={v}>{v}</option>)}
      </select>
    </>
  );
}

function PackContainsEditor({ clause, onChange }: ClauseEditorProps<ClauseAntePackContains>) {
  return (
    <>
      <AnteSelect value={clause.ante} onChange={v => onChange({ ...clause, ante: v })} />
      <label className="clause-label">Pack #</label>
      <select
        className="input-select"
        value={clause.pack_index}
        onChange={e => onChange({ ...clause, pack_index: Number(e.target.value) })}
        aria-label="Pack index"
      >
        {[0, 1, 2, 3].map(p => <option key={p} value={p}>Pack {p + 1}</option>)}
      </select>
      <label className="clause-label">Card</label>
      <select
        className="input-select input-select--wide"
        value={clause.card}
        onChange={e => onChange({ ...clause, card: e.target.value })}
        aria-label="Card"
      >
        {CARDS.map(c => <option key={c} value={c}>{c}</option>)}
      </select>
    </>
  );
}

// ─── Clause row ──────────────────────────────────────────────────────────────

const CLAUSE_KIND_LABELS: Record<FilterClause['kind'], string> = {
  ante_shop_has_joker: 'Shop has joker',
  ante_tag_is: 'Tag is',
  ante_boss_is: 'Boss is',
  voucher_is: 'Voucher is',
  ante_pack_contains: 'Pack contains',
};

type ClauseRowProps = {
  clause: FilterClause;
  index: number;
  onChange: (index: number, clause: FilterClause) => void;
  onRemove: (index: number) => void;
};

function ClauseRow({ clause, index, onChange, onRemove }: ClauseRowProps) {
  const handleKindChange = (kind: FilterClause['kind']) => {
    onChange(index, defaultClause(kind));
  };

  return (
    <div className="clause-row">
      <select
        className="input-select input-select--kind"
        value={clause.kind}
        onChange={e => handleKindChange(e.target.value as FilterClause['kind'])}
        aria-label="Clause type"
      >
        {Object.entries(CLAUSE_KIND_LABELS).map(([k, label]) => (
          <option key={k} value={k}>{label}</option>
        ))}
      </select>

      {clause.kind === 'ante_shop_has_joker' && (
        <ShopHasJokerEditor
          clause={clause}
          onChange={updated => onChange(index, updated)}
        />
      )}
      {clause.kind === 'ante_tag_is' && (
        <TagIsEditor
          clause={clause}
          onChange={updated => onChange(index, updated)}
        />
      )}
      {clause.kind === 'ante_boss_is' && (
        <BossIsEditor
          clause={clause}
          onChange={updated => onChange(index, updated)}
        />
      )}
      {clause.kind === 'voucher_is' && (
        <VoucherIsEditor
          clause={clause}
          onChange={updated => onChange(index, updated)}
        />
      )}
      {clause.kind === 'ante_pack_contains' && (
        <PackContainsEditor
          clause={clause}
          onChange={updated => onChange(index, updated)}
        />
      )}

      <button
        className="btn btn--icon btn--danger"
        onClick={() => onRemove(index)}
        aria-label="Remove clause"
        title="Remove clause"
      >
        ✕
      </button>
    </div>
  );
}

// ─── FilterBuilder component ─────────────────────────────────────────────────

export type FilterBuilderProps = {
  filter: Filter;
  onChange: (filter: Filter) => void;
};

export function FilterBuilder({ filter, onChange }: FilterBuilderProps) {
  const addClause = useCallback(() => {
    onChange({
      ...filter,
      clauses: [...filter.clauses, defaultClause('ante_shop_has_joker')],
    });
  }, [filter, onChange]);

  const updateClause = useCallback(
    (index: number, clause: FilterClause) => {
      const clauses = [...filter.clauses];
      clauses[index] = clause;
      onChange({ ...filter, clauses });
    },
    [filter, onChange],
  );

  const removeClause = useCallback(
    (index: number) => {
      const clauses = filter.clauses.filter((_, i) => i !== index);
      onChange({ ...filter, clauses });
    },
    [filter, onChange],
  );

  const loadPreset = useCallback(
    (presetId: string) => {
      const preset = PRESETS.find(p => p.id === presetId);
      if (preset) onChange({ ...preset.filter });
    },
    [onChange],
  );

  return (
    <div className="filter-builder card">
      <div className="card-header">
        <h2 className="card-title">Filter</h2>
        <div className="preset-row">
          <label className="clause-label" htmlFor="preset-select">Preset:</label>
          <select
            id="preset-select"
            className="input-select input-select--wide"
            defaultValue=""
            onChange={e => {
              if (e.target.value) loadPreset(e.target.value);
            }}
          >
            <option value="" disabled>Load a preset…</option>
            {PRESETS.map(p => (
              <option key={p.id} value={p.id}>{p.label}</option>
            ))}
          </select>
        </div>
      </div>

      <div className="clause-list">
        {filter.clauses.length === 0 && (
          <p className="empty-hint">No clauses yet. Add one below to start filtering.</p>
        )}
        {filter.clauses.map((clause, i) => (
          <ClauseRow
            key={i}
            clause={clause}
            index={i}
            onChange={updateClause}
            onRemove={removeClause}
          />
        ))}
      </div>

      <button className="btn btn--secondary btn--add" onClick={addClause}>
        + Add clause
      </button>

      <div className="filter-options">
        <label className="toggle-label">
          <input
            type="checkbox"
            className="toggle-input"
            checked={filter.partial}
            onChange={e => onChange({ ...filter, partial: e.target.checked })}
          />
          <span className="toggle-track">
            <span className="toggle-thumb" />
          </span>
          <span className="toggle-text">
            {filter.partial ? 'Partial match' : 'Strict match'}
          </span>
        </label>

        {filter.partial && (
          <div className="score-slider">
            <label className="clause-label" htmlFor="min-score">
              Min score: <strong>{filter.min_score ?? 0}</strong>
            </label>
            <input
              id="min-score"
              type="range"
              min={0}
              max={10}
              step={1}
              value={filter.min_score ?? 0}
              onChange={e =>
                onChange({ ...filter, min_score: Number(e.target.value) })
              }
              className="range-input"
            />
          </div>
        )}
      </div>
    </div>
  );
}
