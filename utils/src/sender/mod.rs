use codec::Decode;
use futures::future::try_join_all;
use log::*;
use subxt::{
	extrinsic::Era,
	sp_core::{sr25519::Pair as SrPair, Pair},
	ClientBuilder, DefaultConfig, PairSigner, PolkadotExtrinsicParams,
	PolkadotExtrinsicParamsBuilder as Params,
};

#[subxt::subxt(runtime_metadata_path = "metadata.scale")]
pub mod runtime {}

async fn wait_for_events(node: String, n: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let api = ClientBuilder::new()
		.set_url(node)
		.build()
		.await?
		.to_runtime_api::<runtime::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>>(
		);

	let mut balance_transfer_count = 0;
	let mut last_checked_block_number = 0;

	let mut finalized_block_headers = api.client.rpc().subscribe_finalized_blocks().await?;

	while let Some(b) = finalized_block_headers.next().await {
		let finalized_block_header = b.unwrap();
		let finalized_block_number = finalized_block_header.number;

		if finalized_block_number <= last_checked_block_number {
			warn!("Ignored past block: {finalized_block_number}");
			continue
		}

		for i in last_checked_block_number..finalized_block_number {
			let block_hash = api.client.rpc().block_hash(Some(i.into())).await?.unwrap();

			let events = api.events().at(block_hash).await?;
			for e in events.iter_raw() {
				if let Ok(raw_event) = e {
					if raw_event.pallet == "Balances" && raw_event.variant == "Transfer" {
						balance_transfer_count = balance_transfer_count + 1;
					}
				}
			}
		}

		last_checked_block_number = finalized_block_number;

		if balance_transfer_count >= n {
            info!("Found all {} transfer events", balance_transfer_count);
			break
		}
        if balance_transfer_count > 0 {
            info!("Found {} transfer events, need {} more", balance_transfer_count, n - balance_transfer_count);
        }
	}

	Ok(())
}

pub async fn send_funds(
	node: String,
	derivation: &str,
	chunk_size: usize,
	n: usize,
) -> Result<(), Box<dyn std::error::Error>> {
	let receivers = generate_receivers(n); // one receiver per sender

	let api = ClientBuilder::new()
		.set_url(node.clone())
		.build()
		.await?
		.to_runtime_api::<runtime::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>>(
		);

	let ext_deposit = api.constants().balances().existential_deposit().unwrap();

	info!("Signing {} transactions", n);
	let mut txs = Vec::new();
	for i in 0..n {
		let signer = generate_signer(derivation, i);
		let tx_params = Params::new().era(Era::Immortal, *api.client.genesis());
		let tx = api
			.tx()
			.balances()
			.transfer_keep_alive(receivers[i as usize].clone().into(), ext_deposit)
			.create_signed(&signer, tx_params)
			.await?;
		txs.push(tx);
	}

    // Start a second thread to listen for `Transfer` events.
    let wait_for_events = tokio::task::spawn(async move {
	    wait_for_events(node, n).await
	});

	info!("Sending {} transactions in chunks of {}", n, chunk_size);
	let mut i = 0;
	let mut last_now = std::time::Instant::now();
	let mut last_sent = 0;
	let start = std::time::Instant::now();

	for chunk in txs.chunks(chunk_size) {
		let mut hashes = Vec::new();
		for tx in chunk {
			let hash = api.client.rpc().submit_extrinsic(tx);
			hashes.push(hash);
		}
		try_join_all(hashes).await?;

		let elapsed = last_now.elapsed();
		if elapsed >= std::time::Duration::from_secs(1) {
			let sent = i * chunk_size - last_sent;
			let rate = sent as f64 / elapsed.as_secs_f64();
			info!("{} txs sent in {} ms ({:.2} /s)", sent, elapsed.as_millis(), rate);
			last_now = std::time::Instant::now();
			last_sent = i * chunk_size;
		}
		i += 1;
	}
	let rate = n as f64 / start.elapsed().as_secs_f64();
	info!("{} txs sent in {} ms ({:.2} /s)", n, start.elapsed().as_millis(), rate);

    // Wait until all `Transfer` events were received.
    // Any timeout can be handled by the Zombienet DSL.
	wait_for_events.await?.map_err(|e| format!("Failed to wait for events: {:?}", e))?;
    Ok(())
}

pub fn generate_signer(derivation_blueprint: &str, i: usize) -> PairSigner<DefaultConfig, SrPair> {
	let pair: SrPair =
		Pair::from_string(format!("{}{}", derivation_blueprint, i).as_str(), None).unwrap();
	let signer: PairSigner<DefaultConfig, SrPair> = PairSigner::new(pair);
	signer
}

/// Generates a vector of account IDs.
fn generate_receivers(num: usize) -> Vec<subxt::sp_core::crypto::AccountId32> {
	let mut receivers = Vec::new();
	for i in 0..num {
		// Decode the account ID from the string:
		let account_id = Decode::decode(&mut &format!("{:0>32?}", i).as_bytes()[..])
			.expect("Must decode account ID");
		receivers.push(account_id);
	}
	debug!("Generated {} receiver addresses", receivers.len());
	receivers
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeSet as Set;

	#[test]
	/// Check that the generated addresses are unique.
	fn generate_receivers_unique() {
		let receivers = super::generate_receivers(1024);
		let set: Set<_> = receivers.iter().collect();

		assert_eq!(set.len(), receivers.len());
	}
}
