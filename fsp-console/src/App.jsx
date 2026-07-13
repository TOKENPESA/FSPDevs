import { ProfileProvider } from "./context/ProfileContext.jsx";
import SuperAppConsole from "./components/superapp/SuperAppConsole.jsx";
import "./index.css";

export default function App() {
  return (
    <ProfileProvider>
      <SuperAppConsole />
    </ProfileProvider>
  );
}
