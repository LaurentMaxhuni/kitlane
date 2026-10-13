-- Development only. Fixture rows roll back inside an exception subtransaction.
DO $tests$
DECLARE
  a uuid := gen_random_uuid(); b uuid := gen_random_uuid();
  d1 uuid := '11111111-1111-4111-8111-111111111111';
  d2 uuid := '22222222-2222-4222-8222-222222222222';
BEGIN
  BEGIN
    INSERT INTO neon_auth."user"(id,name,email,"emailVerified","createdAt","updatedAt")
      VALUES (a,'Constraint test',a::text||'@test.invalid',false,now(),now()),
             (b,'Constraint test',b::text||'@test.invalid',false,now(),now());
    INSERT INTO kitlane.accounts VALUES (a),(b);
    INSERT INTO kitlane.devices(account_id,device_id,name,platform,architecture) VALUES (a,d1,'A','windows','x86_64'),(a,d2,'B','windows','x86_64'),(b,d1,'Foreign','linux','x86_64');
    BEGIN
      INSERT INTO kitlane.pairs VALUES (b,d1,d2);
      RAISE EXCEPTION 'cross-account pair accepted';
    EXCEPTION WHEN foreign_key_violation THEN NULL; END;
    BEGIN
      INSERT INTO kitlane.pairs VALUES (a,d1,d1);
      RAISE EXCEPTION 'self pair accepted';
    EXCEPTION WHEN check_violation THEN NULL; END;
    INSERT INTO kitlane.pairs VALUES (a,d1,d2);
    BEGIN
      INSERT INTO kitlane.pairs VALUES (a,d1,d2);
      RAISE EXCEPTION 'duplicate pair accepted';
    EXCEPTION WHEN unique_violation THEN NULL; END;
    BEGIN
      INSERT INTO kitlane.snapshots VALUES (b,d2,'{}');
      RAISE EXCEPTION 'foreign snapshot accepted';
    EXCEPTION WHEN foreign_key_violation THEN NULL; END;
    RAISE SQLSTATE 'K0001';
  EXCEPTION WHEN SQLSTATE 'K0001' THEN NULL;
  END;
END $tests$;
