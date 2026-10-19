import { NotebookNavigation } from "@/components/notebook/NotebookNavigation";
import { useEffect, useRef } from "react";
import {startSnapshotCacheMaintenance} from "@/stores/snapshotCacheMaintenance";
import { Routes, Route, Navigate, useLocation, useNavigate } from "react-router-dom";
import { AppShell } from "@/components/layout/AppShell";
import { PlatformView } from "@/pages/PlatformView";
import { CentralSkillsView } from "@/pages/CentralSkillsView";
import { ResourceLibraryView } from "@/pages/ResourceLibraryView";
import { SkillDetailPage } from "@/pages/SkillDetailPage";
import { SettingsView } from "@/pages/SettingsView";

function App() {
  useEffect(startSnapshotCacheMaintenance, []);
  const location = useLocation();
  const navigate = useNavigate();
  const settingsOpen = location.pathname === "/settings";
  const previousPage = useRef({...location, pathname: "/resources", search: "", hash: ""});
  if (!settingsOpen) previousPage.current = location;
  const background = previousPage.current;
  return (
    <><NotebookNavigation/><Routes location={settingsOpen ? background : location}>
      <Route path="/" element={<AppShell settingsOpen={settingsOpen} onCloseSettings={() => navigate({pathname:background.pathname, search:background.search, hash:background.hash}, {replace:true, state:background.state})} />}>
        {/* Default redirect to Skill Repository */}
        <Route index element={<Navigate to="/resources" replace />} />
        {/* Platform view: lists skills for a specific agent */}
        <Route path="platform/:agentId" element={<PlatformView />} />
        {/* Shared Hub: canonical ~/.agents/skills/ view */}
        <Route path="central" element={<CentralSkillsView />} />
        {/* Skill Repository: downloaded/imported skill source library */}
        <Route path="resources" element={<ResourceLibraryView />} />
        {/* Skill detail page */}
        <Route path="skill/:skillId" element={<SkillDetailPage />} />
        {/* Skill Bundles */}
        <Route path="collections" element={<Navigate to="/resources" replace />} />
        {/* Settings */}
        <Route path="settings" element={<SettingsView />} />
      </Route>
    </Routes></>
  );
}

export default App;
