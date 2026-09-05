import { NextRequest, NextResponse } from "next/server";
export async function GET(req: NextRequest) {
  const date=req.nextUrl.searchParams.get("date");
  if(!date||!/^\d{4}-\d{2}-\d{2}$/.test(date)) return NextResponse.json({error:"Invalid date"},{status:400});
  const url=new URL("https://archive-api.open-meteo.com/v1/archive");
  url.search=new URLSearchParams({latitude:"31.2304",longitude:"121.4737",start_date:date,end_date:date,daily:"precipitation_sum",timezone:"Asia/Shanghai"}).toString();
  const response=await fetch(url,{next:{revalidate:3600}}); const json=await response.json(); const precipitationMm=json?.daily?.precipitation_sum?.[0];
  if(!response.ok||typeof precipitationMm!=="number") return NextResponse.json({error:"Observation unavailable"},{status:502});
  return NextResponse.json({source:"Open-Meteo",date,latitude:31.2304,longitude:121.4737,precipitationMm,observedAt:Math.floor(Date.now()/1000)});
}
