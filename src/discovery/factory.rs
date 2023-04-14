use std::{collections::HashMap, sync::Arc};

use ethers::{
    providers::Middleware,
    types::{Filter, H160, H256},
};
use spinoff::{spinners, Color, Spinner};

use crate::{
    amm::{self, factory::Factory},
    errors::DAMMError,
};

pub enum DiscoverableFactory {
    UniswapV2Factory,
    UniswapV3Factory,
}

impl DiscoverableFactory {
    pub fn discovery_event_signature(&self) -> H256 {
        match self {
            DiscoverableFactory::UniswapV2Factory => {
                amm::uniswap_v2::factory::PAIR_CREATED_EVENT_SIGNATURE
            }

            DiscoverableFactory::UniswapV3Factory => {
                amm::uniswap_v3::factory::POOL_CREATED_EVENT_SIGNATURE
            }
        }
    }
}

// Returns a vec of empty factories that match one of the Factory interfaces specified by each DiscoverableFactory
pub async fn discover_factories<M: Middleware>(
    factories: Vec<DiscoverableFactory>,
    number_of_amms_threshold: u64,
    middleware: Arc<M>,
) -> Result<Vec<Factory>, DAMMError<M>> {
    let spinner = Spinner::new(spinners::Dots, "Discovering new factories...", Color::Blue);

    let mut event_signatures = vec![];

    for factory in factories {
        event_signatures.push(factory.discovery_event_signature());
    }

    let block_filter = Filter::new().topic0(event_signatures);

    let from_block = 0;
    let current_block = middleware
        .get_block_number()
        .await
        .map_err(DAMMError::MiddlewareError)?
        .as_u64();

    //For each block within the range, get all pairs asynchronously
    let step = 100000;

    //Set up filter and events to filter each block you are searching by
    let mut identified_factories: HashMap<H160, (Factory, u64)> = HashMap::new();

    for from_block in (from_block..=current_block).step_by(step) {
        //Get pair created event logs within the block range
        let mut to_block = from_block + step as u64;
        if to_block > current_block {
            to_block = current_block;
        }

        let block_filter = block_filter.clone();
        let logs = middleware
            .get_logs(&block_filter.from_block(from_block).to_block(to_block))
            .await
            .map_err(DAMMError::MiddlewareError)?;

        for log in logs {
            if let Some((_, amms_length)) = identified_factories.get_mut(&log.address) {
                *amms_length += 1;
            } else {
                //TODO: conduct interface checks for the given factory

                let factory = Factory::new_empty_factory_from_event_signature(log.topics[0]);

                match factory {
                    Factory::UniswapV2Factory(mut uniswap_v2_factory) => {
                        uniswap_v2_factory.address = log.address
                    }
                    Factory::UniswapV3Factory(mut uniswap_v3_factory) => {
                        uniswap_v3_factory.address = log.address
                    }
                }

                identified_factories.insert(log.address, (factory, 0));
            }
        }
    }

    let mut filtered_factories = vec![];
    for (_, (factory, amms_length)) in identified_factories {
        if amms_length >= number_of_amms_threshold {
            filtered_factories.push(factory);
        }
    }

    spinner.success("All factories discovered");
    Ok(filtered_factories)
}