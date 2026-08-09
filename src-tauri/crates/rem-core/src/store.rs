use rusqlite::types::Type;
use rusqlite::{params, Connection, Row, Transaction};
use uuid::Uuid;

use crate::collection::write_transaction;
use crate::tags::normalize_card_tags;
use crate::{
    normalize_user_tags, Card, CardPatch, Collection, CollectionError, CreateCardsResult, Deck,
    DeckPatch, DeckSettings, DuplicatePolicy, FsrsState, NewCardInput, SchedulerKind,
};

const DECK_PALETTE: [&str; 5] = ["#7e6cff", "#e8638c", "#2fa86b", "#e8922e", "#3ba0e8"];

impl Collection {
    pub fn create_deck(&self, name: &str, now: i64) -> Result<Deck, CollectionError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CollectionError::InvalidInput(
                "deck name must not be blank".into(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        let deck = Deck {
            id: id.clone(),
            name: name.to_owned(),
            created_at: now,
            updated_at: now,
            color: deck_color(&id),
            scheduler_kind: SchedulerKind::Fsrs,
            settings: DeckSettings::default(),
        };
        let settings = serde_json::to_string(&deck.settings)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        transaction.execute(
            "INSERT INTO decks(
                id, name, created_at, updated_at, color, scheduler_kind, settings_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'fsrs', ?6)",
            params![
                deck.id,
                deck.name,
                deck.created_at,
                deck.updated_at,
                deck.color,
                settings
            ],
        )?;
        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok(deck)
    }

    pub fn update_deck(&self, id: &str, patch: DeckPatch, now: i64) -> Result<(), CollectionError> {
        let name = patch.name.map(|name| name.trim().to_owned());
        if name.as_deref() == Some("") {
            return Err(CollectionError::InvalidInput(
                "deck name must not be blank".into(),
            ));
        }
        let settings = patch
            .settings
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let changed = transaction.execute(
            "UPDATE decks SET
                name = COALESCE(?2, name),
                color = COALESCE(?3, color),
                settings_json = COALESCE(?4, settings_json),
                updated_at = ?5
             WHERE id = ?1",
            params![id, name, patch.color, settings, now],
        )?;
        if changed == 0 {
            return Err(CollectionError::NotFound {
                kind: "deck",
                id: id.to_owned(),
            });
        }
        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn create_card(
        &self,
        deck_id: &str,
        front: &str,
        back: &str,
        tags: Vec<String>,
        now: i64,
    ) -> Result<Card, CollectionError> {
        let result = self.create_cards(
            deck_id,
            vec![NewCardInput {
                front: front.to_owned(),
                back: back.to_owned(),
                tags,
            }],
            now,
            DuplicatePolicy::Allow,
        )?;
        Ok(result.created.into_iter().next().expect("one card created"))
    }

    pub fn create_cards(
        &self,
        deck_id: &str,
        inputs: Vec<NewCardInput>,
        now: i64,
        duplicate_policy: DuplicatePolicy,
    ) -> Result<CreateCardsResult, CollectionError> {
        let cards = prepare_cards(deck_id, inputs, now)?;

        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        if !deck_exists(&transaction, deck_id)? {
            return Err(CollectionError::NotFound {
                kind: "deck",
                id: deck_id.to_owned(),
            });
        }

        let mut result = CreateCardsResult {
            created: Vec::new(),
            duplicates: Vec::new(),
        };
        for card in cards {
            let tags_json = serde_json::to_string(&card.tags)?;
            if duplicate_policy == DuplicatePolicy::Skip {
                if let Some(existing) =
                    find_exact_card(&transaction, deck_id, &card.front, &card.back, &tags_json)?
                {
                    result.duplicates.push(existing);
                    continue;
                }
            }
            let scheduling_json = serde_json::to_string(&card.scheduling)?;
            insert_card(&transaction, &card, &tags_json, &scheduling_json)?;
            result.created.push(card);
        }
        if !result.created.is_empty() {
            bump_sync_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok(result)
    }

    pub fn preview_cards(
        &self,
        deck_id: &str,
        inputs: Vec<NewCardInput>,
        now: i64,
        duplicate_policy: DuplicatePolicy,
    ) -> Result<CreateCardsResult, CollectionError> {
        let cards = prepare_cards(deck_id, inputs, now)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        if !deck_exists(&connection, deck_id)? {
            return Err(CollectionError::NotFound {
                kind: "deck",
                id: deck_id.to_owned(),
            });
        }

        let mut result = CreateCardsResult {
            created: Vec::new(),
            duplicates: Vec::new(),
        };
        for card in cards {
            if duplicate_policy == DuplicatePolicy::Skip {
                let tags_json = serde_json::to_string(&card.tags)?;
                let duplicate =
                    find_exact_card(&connection, deck_id, &card.front, &card.back, &tags_json)?
                        .or_else(|| {
                            result
                                .created
                                .iter()
                                .find(|planned| same_content(planned, &card))
                                .cloned()
                        });
                if let Some(duplicate) = duplicate {
                    result.duplicates.push(duplicate);
                    continue;
                }
            }
            result.created.push(card);
        }
        Ok(result)
    }

    pub fn update_card(&self, id: &str, patch: CardPatch, now: i64) -> Result<(), CollectionError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let transaction = write_transaction(&mut connection)?;
        let mut card =
            find_card_by_id(&transaction, id)?.ok_or_else(|| CollectionError::NotFound {
                kind: "card",
                id: id.to_owned(),
            })?;
        apply_card_patch(&mut card, patch, now)?;
        let tags_json = serde_json::to_string(&card.tags)?;
        let scheduling_json = serde_json::to_string(&card.scheduling)?;
        update_card_row(&transaction, &card, &tags_json, &scheduling_json)?;
        bump_sync_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn due_cards(&self, deck_id: &str, now: i64) -> Result<Vec<Card>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, deck_id, front, back, created_at, updated_at, tags_json,
                    suspended, last_again_at, scheduling_json, due
             FROM cards
             WHERE deck_id = ?1 AND suspended = 0 AND due <= ?2
             ORDER BY due, rowid",
        )?;
        let cards = statement
            .query_map(params![deck_id, now], card_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(cards)
    }

    pub fn count_due(&self, deck_id: &str, now: i64) -> Result<u64, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let count = connection.query_row(
            "SELECT COUNT(*) FROM cards
             WHERE deck_id = ?1 AND suspended = 0 AND due <= ?2",
            params![deck_id, now],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn get_card(&self, id: &str) -> Result<Option<Card>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, deck_id, front, back, created_at, updated_at, tags_json,
                    suspended, last_again_at, scheduling_json, due
             FROM cards WHERE id = ?1",
        )?;
        let mut rows = statement.query([id])?;
        rows.next()?
            .map(card_from_row)
            .transpose()
            .map_err(Into::into)
    }

    pub fn list_cards(&self, deck_id: &str) -> Result<Vec<Card>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, deck_id, front, back, created_at, updated_at, tags_json,
                    suspended, last_again_at, scheduling_json, due
             FROM cards WHERE deck_id = ?1 ORDER BY created_at, rowid",
        )?;
        let cards = statement
            .query_map([deck_id], card_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(cards)
    }

    pub fn get_deck(&self, id: &str) -> Result<Option<Deck>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, name, created_at, updated_at, color, scheduler_kind, settings_json
             FROM decks WHERE id = ?1",
        )?;
        let mut rows = statement.query([id])?;
        rows.next()?
            .map(deck_from_row)
            .transpose()
            .map_err(Into::into)
    }

    pub fn list_decks(&self) -> Result<Vec<Deck>, CollectionError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| CollectionError::Poisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, name, created_at, updated_at, color, scheduler_kind, settings_json
             FROM decks ORDER BY created_at, rowid",
        )?;
        let decks = statement
            .query_map([], deck_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(decks)
    }
}

fn prepare_cards(
    deck_id: &str,
    inputs: Vec<NewCardInput>,
    now: i64,
) -> Result<Vec<Card>, CollectionError> {
    inputs
        .into_iter()
        .map(|input| {
            if input.front.trim().is_empty() {
                return Err(CollectionError::InvalidInput(
                    "card front must not be blank".into(),
                ));
            }
            Ok(Card {
                id: Uuid::new_v4().to_string(),
                deck_id: deck_id.to_owned(),
                front: input.front,
                back: input.back,
                created_at: now,
                updated_at: now,
                tags: normalize_user_tags(input.tags),
                suspended: false,
                last_again_at: None,
                scheduling: FsrsState::initial(now),
            })
        })
        .collect()
}

fn same_content(left: &Card, right: &Card) -> bool {
    left.deck_id == right.deck_id
        && left.front == right.front
        && left.back == right.back
        && left.tags == right.tags
}

pub(crate) fn apply_card_patch(
    card: &mut Card,
    patch: CardPatch,
    now: i64,
) -> Result<(), CollectionError> {
    if let Some(front) = patch.front {
        if front.trim().is_empty() {
            return Err(CollectionError::InvalidInput(
                "card front must not be blank".into(),
            ));
        }
        card.front = front;
    }
    if let Some(back) = patch.back {
        card.back = back;
    }
    if let Some(tags) = patch.tags {
        card.tags = normalize_card_tags(tags);
    }
    if let Some(suspended) = patch.suspended {
        card.suspended = suspended;
    }
    if let Some(last_again_at) = patch.last_again_at {
        card.last_again_at = last_again_at;
    }
    if let Some(scheduling) = patch.scheduling {
        card.scheduling = scheduling;
    }
    card.updated_at = now;
    Ok(())
}

pub(crate) fn find_card_by_id(
    transaction: &Transaction<'_>,
    id: &str,
) -> rusqlite::Result<Option<Card>> {
    let mut statement = transaction.prepare(
        "SELECT id, deck_id, front, back, created_at, updated_at, tags_json,
                suspended, last_again_at, scheduling_json, due
         FROM cards WHERE id = ?1",
    )?;
    let mut rows = statement.query([id])?;
    rows.next()?.map(card_from_row).transpose()
}

fn find_exact_card(
    connection: &Connection,
    deck_id: &str,
    front: &str,
    back: &str,
    tags_json: &str,
) -> rusqlite::Result<Option<Card>> {
    let mut statement = connection.prepare(
        "SELECT id, deck_id, front, back, created_at, updated_at, tags_json,
                suspended, last_again_at, scheduling_json, due
         FROM cards
         WHERE deck_id = ?1 AND front = ?2 AND back = ?3 AND tags_json = ?4
         ORDER BY created_at, rowid LIMIT 1",
    )?;
    let mut rows = statement.query(params![deck_id, front, back, tags_json])?;
    rows.next()?.map(card_from_row).transpose()
}

pub(crate) fn update_card_row(
    transaction: &Transaction<'_>,
    card: &Card,
    tags_json: &str,
    scheduling_json: &str,
) -> rusqlite::Result<()> {
    transaction.execute(
        "UPDATE cards SET
            front = ?2, back = ?3, updated_at = ?4, tags_json = ?5,
            suspended = ?6, last_again_at = ?7, scheduling_json = ?8, due = ?9
         WHERE id = ?1",
        params![
            card.id,
            card.front,
            card.back,
            card.updated_at,
            tags_json,
            card.suspended,
            card.last_again_at,
            scheduling_json,
            card.scheduling.due,
        ],
    )?;
    Ok(())
}

pub(crate) fn insert_card(
    transaction: &Transaction<'_>,
    card: &Card,
    tags_json: &str,
    scheduling_json: &str,
) -> rusqlite::Result<()> {
    transaction.execute(
        "INSERT INTO cards(
            id, deck_id, front, back, created_at, updated_at, tags_json,
            suspended, last_again_at, scheduling_json, due
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            card.id,
            card.deck_id,
            card.front,
            card.back,
            card.created_at,
            card.updated_at,
            tags_json,
            card.suspended,
            card.last_again_at,
            scheduling_json,
            card.scheduling.due,
        ],
    )?;
    Ok(())
}

fn deck_exists(connection: &Connection, deck_id: &str) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM decks WHERE id = ?1)",
        [deck_id],
        |row| row.get(0),
    )
}

pub(crate) fn bump_sync_revision(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute(
        "UPDATE metadata SET value = value + 1 WHERE key = 'sync_revision'",
        [],
    )?;
    Ok(())
}

pub(crate) fn card_from_row(row: &Row<'_>) -> rusqlite::Result<Card> {
    let tags_json: String = row.get(6)?;
    let tags = serde_json::from_str(&tags_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, Type::Text, Box::new(error))
    })?;
    let scheduling_json: String = row.get(9)?;
    let scheduling: FsrsState = serde_json::from_str(&scheduling_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(9, Type::Text, Box::new(error))
    })?;
    let due: i64 = row.get(10)?;
    if scheduling.due != due {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            10,
            Type::Integer,
            "indexed due time does not match scheduling state".into(),
        ));
    }
    Ok(Card {
        id: row.get(0)?,
        deck_id: row.get(1)?,
        front: row.get(2)?,
        back: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        tags,
        suspended: row.get(7)?,
        last_again_at: row.get(8)?,
        scheduling,
    })
}

pub(crate) fn deck_from_row(row: &Row<'_>) -> rusqlite::Result<Deck> {
    let scheduler_kind: String = row.get(5)?;
    if scheduler_kind != "fsrs" {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            5,
            Type::Text,
            format!("unsupported scheduler kind: {scheduler_kind}").into(),
        ));
    }
    let settings_json: String = row.get(6)?;
    let settings = serde_json::from_str(&settings_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, Type::Text, Box::new(error))
    })?;
    Ok(Deck {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        color: row.get(4)?,
        scheduler_kind: SchedulerKind::Fsrs,
        settings,
    })
}

pub(crate) fn deck_color(id: &str) -> String {
    let hash = id.bytes().fold(0_u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as u32)
    });
    DECK_PALETTE[hash as usize % DECK_PALETTE.len()].to_owned()
}
