package com.andyhui.cpime

import android.content.Context
import java.io.File
import java.io.FileOutputStream

/**
 * 資料庫資產複製助手：
 * 首次啟動時將唯讀系統詞庫 `assets/cpime_dict.db` 複製到 App 私有目錄 `filesDir/cpime_dict.db`，
 * 並規劃可讀寫的個人學習詞庫路徑 `filesDir/user_dict.db`。
 */
object AssetHelper {
    private const val SYS_DB_NAME = "cpime_dict.db"
    private const val USER_DB_NAME = "user_dict.db"
    private const val BUFFER_SIZE = 64 * 1024

    data class DatabasePaths(
        val sysDbPath: String,
        val userDbPath: String,
    )

    @Synchronized
    fun ensureDatabasesReady(context: Context): DatabasePaths {
        val filesDir = context.filesDir
        if (!filesDir.exists()) {
            filesDir.mkdirs()
        }

        val sysDbFile = File(filesDir, SYS_DB_NAME)
        val userDbFile = File(filesDir, USER_DB_NAME)

        if (!sysDbFile.exists() || sysDbFile.length() == 0L) {
            val tempFile = File(filesDir, "$SYS_DB_NAME.tmp")
            if (tempFile.exists()) {
                tempFile.delete()
            }
            context.assets.open(SYS_DB_NAME).use { input ->
                FileOutputStream(tempFile).use { output ->
                    val buffer = ByteArray(BUFFER_SIZE)
                    while (true) {
                        val read = input.read(buffer)
                        if (read <= 0) break
                        output.write(buffer, 0, read)
                    }
                    output.flush()
                }
            }
            if (sysDbFile.exists()) {
                sysDbFile.delete()
            }
            if (!tempFile.renameTo(sysDbFile)) {
                tempFile.copyTo(sysDbFile, overwrite = true)
                tempFile.delete()
            }
        }

        return DatabasePaths(
            sysDbPath = sysDbFile.absolutePath,
            userDbPath = userDbFile.absolutePath,
        )
    }
}
