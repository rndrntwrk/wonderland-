// Test-only setup, never bundled by production entrypoint.
export { AccountAuthority } from "../src/account/authority";
import { accountHttp, type AccountHttpEnv } from "../src/http/accounts";
export default {
  async fetch(request: Request, env: AccountHttpEnv) {
    if (new URL(request.url).pathname === "/test-setup") {
      const stub = env.ACCOUNTS!.get(
        env.ACCOUNTS!.idFromName(env.NATIVE_ACCOUNT_REALM!),
      );
      const { username, name } = (await request.json()) as {
        username: string;
        name: string;
      };
      const release = "b".repeat(64),
        password = "native-account-test-passphrase";
      await stub.initializeCatalog({
        release,
        entries: [
          { key: "1", part: "head", gender: "female", skins: ["dark"] },
          { key: "2", part: "body", gender: "female", skins: ["dark"] },
        ],
      });
      await stub.register(username, password, "a".repeat(64));
      const session = await stub.login(username, password, "a".repeat(64));
      await stub.createAvatar(session.token, "http_test_create_01", {
        name,
        description: "",
        gender: "female",
        skin: "dark",
        head_key: "1",
        body_key: "2",
        release,
      });
      await stub.logout(session.token);
      return Response.json({ seeded: true });
    }
    return accountHttp(request, env);
  },
};
