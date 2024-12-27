/// A migration which renames the pallet `BagsList` to `VoterList`
pub struct RenameBagsListToVoterList;
impl OnRuntimeUpgrade for RenameBagsListToVoterList {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		frame_support::storage::migration::move_pallet(b"BagsList", b"VoterList");
		Ok(())
	}

	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		frame_support::storage::migration::move_pallet(b"BagsList", b"VoterList");
		frame_support::weights::Weight::MAX
	}
}

use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
pub struct AllEdgewareMigrations;
impl OnRuntimeUpgrade for AllEdgewareMigrations {
	fn on_runtime_upgrade() -> Weight {
		let mut weight = 0;
		frame_support::log::info!("💥 RenameBagsListToVoterList start");
		weight += <RenameBagsListToVoterList as OnRuntimeUpgrade>::on_runtime_upgrade();
		frame_support::log::info!("😎 RenameBagsListToVoterList end");
		weight
	}
}
