import MasterCommandBar from "./components/shared/MasterCommandBar";
import FlightStripTable from "./components/hud/FlightStripTable";
import WeatherGridHUD from "./components/hud/WeatherGridHUD";
import SystemMetrics from "./components/hud/SystemMetrics";
import ThreatMatrix from "./components/defense/ThreatMatrix";

export default function HudApp() {
  return (
    <div className="hud-root">
      <MasterCommandBar rangeKm={150} onRangeChange={() => undefined} showEmergency={false} />
      <div className="hud-body">
        <FlightStripTable />
        <WeatherGridHUD />
        <ThreatMatrix />
        <SystemMetrics />
      </div>
    </div>
  );
}
