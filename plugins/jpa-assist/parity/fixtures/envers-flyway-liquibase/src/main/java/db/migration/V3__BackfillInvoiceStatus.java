package db.migration;

import org.flywaydb.core.api.migration.BaseJavaMigration;
import org.flywaydb.core.api.migration.Context;

public class V3__BackfillInvoiceStatus extends BaseJavaMigration {

    @Override
    public void migrate(Context context) throws Exception {
        try (var statement = context.getConnection().createStatement()) {
            statement.execute("update invoice set status = 'PAID' where total is not null and total = 0");
        }
    }
}
