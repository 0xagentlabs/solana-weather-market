# Weather Market ABI

所有整数均为 little-endian；时间为 Unix 秒；降水为微米（1000 μm = 1 mm）。Program ID 为 `HHkVyAqfGWvZrWuNqw1BwLqnr22Hbiwxp1exdoAxpzug`。

## PDA

- Market：`["market", authority(32), target_day(i64)]`
- Vault：`["vault", market(32)]`
- Position：`["position", market(32), user(32)]`

## 指令

| Tag | 指令 | 参数（tag 后） | 账户顺序 |
|---:|---|---|---|
| 0 | CreateMarket | target_day i64, close_ts i64, observation_ts i64, threshold_um u64, oracle pubkey, reserved u8 | authority `[signer,w]`, market `[w]`, vault `[w]`, system program |
| 1 | PlaceBet | side u8（0=NO, 1=YES）, amount u64 | user `[signer,w]`, market `[w]`, position `[w]`, vault `[w]`, system program |
| 2 | CloseMarket | 无 | authority `[signer]`, market `[w]` |
| 3 | SettleMarket | observed_ts i64, precipitation_um u64 | oracle `[signer]`, market `[w]` |
| 4 | Claim | 无 | user `[signer,w]`, market, position `[w]`, vault `[w]` |
| 5 | Refund | 无 | user `[signer,w]`, market, position `[w]`, vault `[w]` |

`SettleMarket` 要求签名者等于市场 oracle；观测时间须在预期时间 ±24h、不能超过链上时间 5 分钟，并且数据不老于 48h。降水量达到阈值即 YES。未结算且超过 observation_ts 48h 后，可按本金退款。赢家领取 `user_stake × total_pool / winning_pool`，整数向下取整。

## 账户布局

Market（160 bytes）：magic `[0,8)`, authority `[8,40)`, oracle `[40,72)`, target_day 72, close_ts 80, observation_ts 88, threshold_um 96, yes_pool 104, no_pool 112, observed_um 120, settled_at 128, status u8 136（0/1/2）, outcome u8 137, market bump 138, vault bump 139。

Position（80 bytes）：magic `[0,8)`, market `[8,40)`, user `[40,72)`, packed u64 at 72。bit63 是 side，bit62 是 claimed，低 62 位是 amount。

## 错误码

1 InvalidInstruction；2 MissingAccount；3 InvalidSigner；4 InvalidWritable；5 InvalidOwner；6 InvalidPda；7 AlreadyInitialized；8 InvalidTime；9 MarketClosed；10 NotClosed；11 AlreadySettled；12 NotSettled；13 Unauthorized；14 InvalidAmount；15 MathOverflow；16 AlreadyClaimed；17 LosingPosition；18 RefundUnavailable；19 InvalidState。
