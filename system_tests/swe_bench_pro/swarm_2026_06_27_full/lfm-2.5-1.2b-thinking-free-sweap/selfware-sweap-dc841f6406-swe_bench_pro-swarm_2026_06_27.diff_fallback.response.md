diff --git a/UserEmail.validateEmail b/UserEmail.validateEmail
@@ -104,7 +104,7 @@ UserEmail.validateEmail {
  const expires = await db.get(`confirm:byUid:${uid}`);
  if (expires.expires < now) return null;
}
@@ -105,12 +105,12 @@ UserEmail.validateEmail {
  return null;
}