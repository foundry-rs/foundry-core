module.exports = {
  platform: "github",
  repositories: ["foundry-rs/foundry-core"],
  onboarding: false,
  requireConfig: "required",
  allowedCommands: [
    "^env CI=true pnpm install --frozen-lockfile --ignore-scripts$",
    "^env CI=true pnpm run build$",
  ],
};
