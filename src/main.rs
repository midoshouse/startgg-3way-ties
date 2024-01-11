use {
    std::{
        cmp::Ordering::*,
        collections::{
            BTreeMap,
            BTreeSet,
            HashMap,
        },
        iter,
        time::Duration as UDuration,
    },
    graphql_client::GraphQLQuery,
    itertools::Itertools as _,
    serde::{
        Deserialize,
        Serialize,
    },
    tokio::time::{
        Instant,
        sleep_until,
    },
    wheel::traits::ReqwestResponseExt as _,
};

#[derive(Deserialize)]
#[serde(untagged)]
enum IdInner {
    Number(serde_json::Number),
    String(String),
}

impl From<IdInner> for ID {
    fn from(inner: IdInner) -> Self {
        Self(match inner {
            IdInner::Number(n) => n.to_string(),
            IdInner::String(s) => s,
        })
    }
}

/// Workaround for <https://github.com/smashgg/developer-portal/issues/171>
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(from = "IdInner", into = "String")]
struct ID(String);

impl From<ID> for String {
    fn from(ID(s): ID) -> Self {
        s
    }
}

type Int = i64;
type String = std::string::String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "assets/graphql/startgg-schema.json",
    query_path = "assets/graphql/startgg-scores-query.graphql",
    skip_default_scalars, // workaround for https://github.com/smashgg/developer-portal/issues/171
    variables_derives = "Clone, PartialEq, Eq, Hash",
    response_derives = "Debug, Clone",
)]
struct ScoresQuery;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct IdPair(ID, ID);

impl IdPair {
    fn new(a: ID, b: ID) -> Self {
        match a.cmp(&b) {
            Less => Self(a, b),
            Equal => panic!(),
            Greater => Self(b, a),
        }
    }
}

enum Standing {
    Win,
    Loss,
    Tbd,
}

async fn process_page(http_client: &reqwest::Client, next_request: &mut Instant, api_key: &str, event_slug: &str, page: i64, player_names: &mut HashMap<ID, String>, scores: &mut BTreeMap<u64, BTreeMap<IdPair, Standing>>) -> Result<i64, Error> {
    sleep_until(*next_request).await;
    let graphql_client::Response { data, errors, extensions: _ } = http_client.post("https://api.start.gg/gql/alpha")
        .bearer_auth(api_key)
        .json(&ScoresQuery::build_query(scores_query::Variables { event_slug: event_slug.to_owned(), page }))
        .send().await?
        .detailed_error_for_status().await?
        .json_with_text_in_error::<graphql_client::Response<scores_query::ResponseData>>().await?;
    // from https://dev.start.gg/docs/rate-limits
    // “You may not average more than 80 requests per 60 seconds.”
    *next_request = Instant::now() + UDuration::from_millis(60_000 / 80);
    let data = match (data, errors) {
        (Some(_), Some(errors)) if !errors.is_empty() => return Err(Error::GraphQL(errors)),
        (Some(data), _) => data,
        (None, Some(errors)) => return Err(Error::GraphQL(errors)),
        (None, None) => return Err(Error::NoDataNoErrors),
    };
    let scores_query::ResponseData {
        event: Some(scores_query::ScoresQueryEvent {
            sets: Some(scores_query::ScoresQueryEventSets {
                page_info: Some(scores_query::ScoresQueryEventSetsPageInfo {
                    total_pages: Some(total_pages),
                }),
                nodes: Some(sets),
            }),
        }),
    } = data else { return Err(Error::ResponseFormat) };
    for set in sets {
        let Some(scores_query::ScoresQueryEventSetsNodes {
            phase_group: Some(scores_query::ScoresQueryEventSetsNodesPhaseGroup {
                display_identifier: Some(group),
                phase: Some(scores_query::ScoresQueryEventSetsNodesPhaseGroupPhase { name: Some(phase) }),
            }),
            slots: Some(slots),
        }) = set else { return Err(Error::ResponseFormat) };
        if phase != "Groups" { continue }
        let [
            Some(scores_query::ScoresQueryEventSetsNodesSlots {
                entrant: Some(scores_query::ScoresQueryEventSetsNodesSlotsEntrant {
                    participants: Some(participants1),
                }),
                standing: Some(scores_query::ScoresQueryEventSetsNodesSlotsStanding {
                    placement: Some(placement1),
                }),
            }),
            Some(scores_query::ScoresQueryEventSetsNodesSlots {
                entrant: Some(scores_query::ScoresQueryEventSetsNodesSlotsEntrant {
                    participants: Some(participants2),
                }),
                standing: Some(scores_query::ScoresQueryEventSetsNodesSlotsStanding {
                    placement: Some(placement2),
                }),
            }),
        ] = <[_; 2]>::try_from(slots).map_err(|_| Error::ResponseFormat)? else { return Err(Error::ResponseFormat) };
        let [Some(scores_query::ScoresQueryEventSetsNodesSlotsEntrantParticipants {
            id: Some(id1),
            gamer_tag: Some(name1),
        })] = <[_; 1]>::try_from(participants1).map_err(|_| Error::ResponseFormat)? else { return Err(Error::ResponseFormat) };
        let [Some(scores_query::ScoresQueryEventSetsNodesSlotsEntrantParticipants {
            id: Some(id2),
            gamer_tag: Some(name2),
        })] = <[_; 1]>::try_from(participants2).map_err(|_| Error::ResponseFormat)? else { return Err(Error::ResponseFormat) };
        player_names.insert(id1.clone(), name1);
        player_names.insert(id2.clone(), name2);
        let (winner_id, loser_id) = match placement1.cmp(&placement2) {
            Less => (id1, id2),
            Equal => {
                scores.entry(group.parse()?).or_default().insert(IdPair::new(id1, id2), Standing::Tbd);
                continue
            },
            Greater => (id2, id1),
        };
        let ids = IdPair::new(winner_id.clone(), loser_id);
        let first_won = winner_id == ids.0;
        scores.entry(group.parse()?).or_default().insert(ids, if first_won { Standing::Win } else { Standing::Loss });
    }
    Ok(total_pages)
}

#[derive(clap::Parser)]
struct Args {
    api_key: String,
    event_slug: String,
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)] ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("{} GraphQL errors", .0.len())]
    GraphQL(Vec<graphql_client::Error>),
    #[error("GraphQL response returned neither `data` nor `errors`")]
    NoDataNoErrors,
    #[error("no match on response format")]
    ResponseFormat,
}

#[wheel::main]
async fn main(Args { api_key, event_slug }: Args) -> Result<(), Error> {
    let http_client = reqwest::Client::builder()
        .user_agent(concat!("startgg-3way-ties/", env!("CARGO_PKG_VERSION")))
        .timeout(UDuration::from_secs(30))
        .use_rustls_tls()
        .https_only(true)
        .build()?;
    let mut next_request = Instant::now();
    let mut player_names = HashMap::default();
    let mut scores = BTreeMap::default();
    let total_pages = process_page(&http_client, &mut next_request, &api_key, &event_slug, 1, &mut player_names, &mut scores).await?;
    for page in 2..=total_pages {
        process_page(&http_client, &mut next_request, &api_key, &event_slug, page, &mut player_names, &mut scores).await?;
    }
    for (group_name, results) in scores {
        let group_members = results.keys().flat_map(|IdPair(a, b)| [a, b]).collect::<BTreeSet<_>>();
        let mut possible_scores = iter::once(vec![0; group_members.len()]).collect_vec();
        for (IdPair(a, b), standing) in &results {
            match standing {
                Standing::Win => for scores in &mut possible_scores {
                    scores[group_members.iter().position(|&member| member == a).unwrap()] += 1;
                },
                Standing::Loss => for scores in &mut possible_scores {
                    scores[group_members.iter().position(|&member| member == b).unwrap()] += 1;
                },
                Standing::Tbd => {
                    let mut new_possible_scores = possible_scores.clone();
                    for scores in &mut possible_scores {
                        scores[group_members.iter().position(|&member| member == a).unwrap()] += 1;
                    }
                    for scores in &mut new_possible_scores {
                        scores[group_members.iter().position(|&member| member == b).unwrap()] += 1;
                    }
                    possible_scores.extend(new_possible_scores);
                }
            }
        }
        let mut triple_tie_possible = false;
        let mut triple_tie_guaranteed = false;
        for scores in &possible_scores {
            if (0..group_members.len()).any(|score| scores.iter().filter(|&&iter_score| iter_score == score).count() == 3) {
                triple_tie_possible = true;
            } else {
                triple_tie_guaranteed = false;
            }
        }
        if triple_tie_guaranteed {
            println!("\ngroup {group_name}: 3-way tie guaranteed, possible scores:");
        } else if triple_tie_possible {
            println!("\ngroup {group_name}: 3-way tie possible, possible scores:");
        } else {
            println!("\ngroup {group_name}: 3-way tie impossible");
            continue
        }
        for scores in possible_scores {
            println!("{}", group_members.iter().zip_eq(&scores).map(|(id, score)| format!("{}: {score}", player_names[id])).format(", "));
        }
    }
    Ok(())
}
