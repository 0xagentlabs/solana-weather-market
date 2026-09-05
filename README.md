# Rain or Shine

Solana Devnet 上的上海日降雨预测市场 Demo。Pinocchio 链上程序管理 YES/NO 资金池，授权预言机读取 Open-Meteo 日降水量后结算；Next.js dapp 提供钱包连接、市场读取、下注、封盘、结算、领取及退款。

- Program ID: `HHkVyAqfGWvZrWuNqw1BwLqnr22Hbiwxp1exdoAxpzug`
- 网络：Solana Devnet
- 完整 ABI：`docs/ABI.md`
- 使用指南：`docs/项目使用说明书.md`

```bash
cargo test
cargo build-sbf
cd app && pnpm install && pnpm dev
```

本项目仅供演示。Devnet SOL 无经济价值；预言机是受信任角色，系统并非完全去中心化。
