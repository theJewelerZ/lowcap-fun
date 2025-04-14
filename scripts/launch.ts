import * as anchor from "@coral-xyz/anchor";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddress,
} from "@solana/spl-token";
import {
  PublicKey,
  Keypair,
  SystemProgram,
} from "@solana/web3.js";

import idlJson from "../target/idl/lowcapfun.json";

const main = async () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const wallet = provider.wallet as anchor.Wallet;
  const idl = idlJson as unknown as anchor.Idl;
  const programId = new PublicKey("9wfonTvRiPDhLKZLCDWyjuZfJCA8T1ABUreFYPhJWd1Y");

  const program = new anchor.Program(idl, programId, provider);

  console.log("✅ Program loaded!");
  console.log("🔑 Wallet:", wallet.publicKey.toBase58());
};

main();

