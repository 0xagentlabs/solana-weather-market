import "./globals.css";import "@solana/wallet-adapter-react-ui/styles.css";import Providers from "./providers";
export const metadata={title:"Rain or Shine | Shanghai Weather Market",description:"A transparent Solana Devnet weather prediction market demo"};
export default function Layout({children}:{children:React.ReactNode}){return <html lang="en"><body><Providers>{children}</Providers></body></html>}
