package quad_native;

// Ponte pro Google Play Billing (compra real de TP). Chamada do lado Rust
// via JNI em crates/client/src/billing_android.rs (metodo estatico
// `launchPurchase`); o resultado volta pro Rust por `onPurchaseFinished`,
// nativo, implementado la'.
//
// Build Gradle real (crates/client/android/app/), migrado de cargo-quad-apk
// em 03/11/2026 justamente pra resolver a arvore de dependencias da AAR da
// Billing Library (androidx.activity, Google Play Services,
// com.google.android.datatransport). Ver docs/BUILD.md.
//
// Sem lambdas de proposito (classes anonimas em vez de `->`): o d8 deste
// pipeline roda com `--min-api 26`, mas o `min_sdk_version` do app e' 24 —
// lambda vira invokedynamic, que so' o runtime do Android 8+ entende
// nativamente. Evitando lambda, o bytecode funciona ate' no min_sdk
// declarado.
import android.app.Activity;
import android.util.Log;

import com.android.billingclient.api.BillingClient;
import com.android.billingclient.api.BillingClientStateListener;
import com.android.billingclient.api.BillingFlowParams;
import com.android.billingclient.api.BillingResult;
import com.android.billingclient.api.ProductDetails;
import com.android.billingclient.api.Purchase;
import com.android.billingclient.api.PendingPurchasesParams;
import com.android.billingclient.api.PurchasesResponseListener;
import com.android.billingclient.api.PurchasesUpdatedListener;
import com.android.billingclient.api.QueryProductDetailsParams;
import com.android.billingclient.api.QueryProductDetailsResult;
import com.android.billingclient.api.QueryPurchasesParams;

import java.util.Arrays;
import java.util.List;

// The SERVER credits and then CONSUMES every purchase (Android Publisher
// API). This side never acknowledges or consumes anything: doing it here,
// before the server credited, could leave a paid purchase with nothing
// delivered. What it does instead is RE-REPORT purchases Google still lists
// as owned (`recover`): one that was paid but whose credit/consume didn't go
// through gets sent to the server again, which is idempotent.
public class TempestBilling {
    private static final String TAG = "TempestBilling";

    /// Codes sent to Rust in `onPurchaseFinished`: 0 = paid, -2 = pending
    /// (cash/boleto still clearing), anything else = Google's response code.
    private static final int PAGO = 0;
    private static final int PENDENTE = -2;

    private static BillingClient client;

    /// Starts the purchase of `productId` ("tp_pacote_2", "oferta_099"…).
    public static void launchPurchase(final Activity activity, final String productId) {
        // Called from the game's native thread; the BillingClient and its
        // purchase screen belong on the UI thread.
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                comCliente(activity, new Runnable() {
                    @Override
                    public void run() {
                        queryAndLaunch(activity, productId);
                    }
                });
            }
        });
    }

    /// Re-reports every purchase Google still lists as owned (paid but not
    /// consumed by the server yet). Called once after login.
    public static void recover(final Activity activity) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                comCliente(activity, new Runnable() {
                    @Override
                    public void run() {
                        consultaPendentes();
                    }
                });
            }
        });
    }

    private static void comCliente(final Activity activity, final Runnable pronto) {
        if (client != null && client.isReady()) {
            pronto.run();
            return;
        }
        PurchasesUpdatedListener listener = new PurchasesUpdatedListener() {
            @Override
            public void onPurchasesUpdated(BillingResult billingResult, List<Purchase> purchases) {
                int codigo = billingResult.getResponseCode();
                if (codigo == BillingClient.BillingResponseCode.OK && purchases != null) {
                    for (Purchase p : purchases) {
                        handlePurchase(p);
                    }
                } else if (codigo == BillingClient.BillingResponseCode.ITEM_ALREADY_OWNED) {
                    // An earlier purchase of this product was never consumed:
                    // hand it to the server again, it delivers/consumes it.
                    onPurchaseFinished("", "", codigo);
                    consultaPendentes();
                } else {
                    onPurchaseFinished("", "", codigo);
                }
            }
        };
        client = BillingClient.newBuilder(activity)
                .setListener(listener)
                .enablePendingPurchases(
                        PendingPurchasesParams.newBuilder().enableOneTimeProducts().build())
                .build();
        client.startConnection(new BillingClientStateListener() {
            @Override
            public void onBillingSetupFinished(BillingResult billingResult) {
                if (billingResult.getResponseCode() == BillingClient.BillingResponseCode.OK) {
                    pronto.run();
                } else {
                    Log.w(TAG, "startConnection falhou: " + billingResult.getDebugMessage());
                    onPurchaseFinished("", "", billingResult.getResponseCode());
                }
            }

            @Override
            public void onBillingServiceDisconnected() {
                Log.w(TAG, "BillingClient desconectou");
            }
        });
    }

    private static void consultaPendentes() {
        QueryPurchasesParams params = QueryPurchasesParams.newBuilder()
                .setProductType(BillingClient.ProductType.INAPP)
                .build();
        client.queryPurchasesAsync(params, new PurchasesResponseListener() {
            @Override
            public void onQueryPurchasesResponse(BillingResult billingResult, List<Purchase> purchases) {
                if (billingResult.getResponseCode() != BillingClient.BillingResponseCode.OK
                        || purchases == null) {
                    return;
                }
                for (Purchase p : purchases) {
                    if (p.getPurchaseState() == Purchase.PurchaseState.PURCHASED) {
                        handlePurchase(p);
                    }
                }
            }
        });
    }

    private static void queryAndLaunch(final Activity activity, final String productId) {
        QueryProductDetailsParams.Product product = QueryProductDetailsParams.Product.newBuilder()
                .setProductId(productId)
                .setProductType(BillingClient.ProductType.INAPP)
                .build();
        QueryProductDetailsParams params = QueryProductDetailsParams.newBuilder()
                .setProductList(Arrays.asList(product))
                .build();

        client.queryProductDetailsAsync(params, new com.android.billingclient.api.ProductDetailsResponseListener() {
            @Override
            public void onProductDetailsResponse(BillingResult billingResult,
                    QueryProductDetailsResult queryResult) {
                List<ProductDetails> productDetailsList = queryResult.getProductDetailsList();
                if (billingResult.getResponseCode() != BillingClient.BillingResponseCode.OK
                        || productDetailsList.isEmpty()) {
                    Log.w(TAG, "queryProductDetailsAsync sem resultado pra' " + productId);
                    int codigo = billingResult.getResponseCode();
                    onPurchaseFinished(productId, "",
                            codigo == BillingClient.BillingResponseCode.OK ? -1 : codigo);
                    return;
                }
                ProductDetails details = productDetailsList.get(0);
                BillingFlowParams.ProductDetailsParams pdp =
                        BillingFlowParams.ProductDetailsParams.newBuilder()
                                .setProductDetails(details)
                                .build();
                BillingFlowParams flowParams = BillingFlowParams.newBuilder()
                        .setProductDetailsParamsList(Arrays.asList(pdp))
                        .build();
                client.launchBillingFlow(activity, flowParams);
            }
        });
    }

    private static void handlePurchase(final Purchase purchase) {
        List<String> produtos = purchase.getProducts();
        String productId = produtos.isEmpty() ? "" : produtos.get(0);
        String token = purchase.getPurchaseToken();
        int estado = purchase.getPurchaseState();
        onPurchaseFinished(productId, token,
                estado == Purchase.PurchaseState.PURCHASED ? PAGO : PENDENTE);
    }

    // Implementado em Rust: crates/client/src/billing_android.rs
    // (Java_quad_1native_TempestBilling_onPurchaseFinished).
    private static native void onPurchaseFinished(String productId, String purchaseToken, int codigo);
}
