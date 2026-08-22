import { createRouter, createWebHistory } from "vue-router";
import type { RouteRecordRaw } from "vue-router";
import { translate } from "@/i18n";
import { APP_NAME } from "@/config/brand";

const routes: RouteRecordRaw[] = [
  {
    path: "/",
    component: () => import("../layouts/DesktopLayout.vue"),
    children: [
      {
        path: "",
        name: "home",
        component: () => import("../pages/HomePage.vue"),
        meta: { titleKey: "nav.home" },
      },
      {
        path: "transfers",
        name: "transfers",
        component: () => import("../pages/TransfersPage.vue"),
        meta: { titleKey: "nav.transfers" },
      },
      {
        path: "completed",
        redirect: { name: "transfers", query: { filter: "completed" } },
      },
      {
        path: "received",
        name: "received",
        component: () => import("../pages/ReceivedFilesPage.vue"),
        meta: { titleKey: "nav.received" },
      },
      {
        path: "devices",
        name: "devices",
        component: () => import("../pages/DevicesPage.vue"),
        meta: { titleKey: "nav.devices" },
      },
      {
        path: "settings",
        name: "settings",
        component: () => import("../pages/SettingsPage.vue"),
        meta: { titleKey: "nav.settings" },
      },
      {
        path: "about",
        name: "about",
        component: () => import("../pages/AboutPage.vue"),
        meta: { titleKey: "settings.about" },
      },
      {
        path: "help",
        name: "help",
        component: () => import("../pages/HelpPage.vue"),
        meta: { titleKey: "nav.help" },
      },
    ],
  },
  // audit-33: legal documents live outside the desktop shell so phones can
  // read them from the mobile menu.
  {
    path: "/legal/:documentType(privacy|terms|disclaimer)",
    name: "legal-document",
    component: () => import("../pages/LegalDocumentPage.vue"),
    props: true,
    meta: { titleKey: "legal.navigation" },
  },
  {
    path: "/mobile",
    component: () => import("../layouts/MobileLayout.vue"),
    children: [
      {
        path: "",
        name: "mobile-send",
        component: () => import("../pages/MobileSendPage.vue"),
        meta: { titleKey: "mobile.send" },
      },
      {
        path: "transfers",
        name: "mobile-transfers",
        component: () => import("../pages/MobileTransferPage.vue"),
        meta: { titleKey: "mobile.transfers" },
      },
      {
        path: "help",
        name: "mobile-help",
        component: () => import("../pages/MobileHelpPage.vue"),
        meta: { titleKey: "mobileHelp.title" },
      },
    ],
  },
  {
    path: ":pathMatch(.*)*",
    name: "not-found",
    component: () => import("../pages/NotFoundPage.vue"),
    meta: { titleKey: "notFound.title" },
  },
];

const router = createRouter({
  history: createWebHistory(),
  routes,
});

// audit-6: keep the window title in sync with the current view.
router.afterEach((to) => {
  if (typeof document === "undefined") return;
  const key = to.meta.titleKey as string | undefined;
  document.title = key ? `${translate(key)} · ${APP_NAME}` : APP_NAME;
});

export default router;
