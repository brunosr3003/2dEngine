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

import com.android.billingclient.api.AcknowledgePurchaseParams;
import com.android.billingclient.api.BillingClient;
import com.android.billingclient.api.BillingClientStateListener;
import com.android.billingclient.api.BillingFlowParams;
import com.android.billingclient.api.BillingResult;
import com.android.billingclient.api.ProductDetails;
import com.android.billingclient.api.Purchase;
import com.android.billingclient.api.PendingPurchasesParams;
import com.android.billingclient.api.PurchasesUpdatedListener;
import com.android.billingclient.api.QueryProductDetailsParams;
import com.android.billingclient.api.QueryProductDetailsResult;

import java.util.Arrays;
import java.util.List;

public class TempestBilling {
    private static final String TAG = "TempestBilling";

    private static BillingClient client;

    /// Dispara a compra de `productId` (ex. "tp_pacote_2", convencao do
    /// servidor) pra' Activity
    /// dada. Chamado pelo Rust com a MESMA Activity que o miniquad ja' tem
    /// em cache (`native::android::ACTIVITY`) — nao criamos nenhuma nova.
    public static void launchPurchase(final Activity activity, final String productId) {
        // Called from the game's native thread; the BillingClient and its
        // purchase screen belong on the UI thread.
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                launchPurchaseNaUi(activity, productId);
            }
        });
    }

    private static void launchPurchaseNaUi(final Activity activity, final String productId) {
        if (client != null && client.isReady()) {
            queryAndLaunch(activity, productId);
            return;
        }

        PurchasesUpdatedListener listener = new PurchasesUpdatedListener() {
            @Override
            public void onPurchasesUpdated(BillingResult billingResult, List<Purchase> purchases) {
                if (billingResult.getResponseCode() == BillingClient.BillingResponseCode.OK
                        && purchases != null) {
                    for (Purchase p : purchases) {
                        handlePurchase(p);
                    }
                } else {
                    // Cancelado, sem rede, ja' possui, etc — sem produto/token
                    // pra' reportar, so' o codigo de erro do Google.
                    onPurchaseFinished("", "", billingResult.getResponseCode());
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
                    queryAndLaunch(activity, productId);
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
                    onPurchaseFinished(productId, "", billingResult.getResponseCode());
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

        onPurchaseFinished(productId, token, purchase.getPurchaseState());

        // Rede de seguranca: o SERVIDOR e' quem credita TP (confere o token
        // na Android Publisher API) e deveria reconhecer a compra la'. Se
        // por algum motivo isso nao acontecer, o Google estorna sozinho em
        // ate' 3 dias — reconhecer aqui tambem e' idempotente e so' evita
        // esse estorno indevido numa compra que o servidor ja' processou.
        if (purchase.getPurchaseState() == Purchase.PurchaseState.PURCHASED
                && !purchase.isAcknowledged()) {
            AcknowledgePurchaseParams ackParams = AcknowledgePurchaseParams.newBuilder()
                    .setPurchaseToken(token)
                    .build();
            client.acknowledgePurchase(ackParams, new com.android.billingclient.api.AcknowledgePurchaseResponseListener() {
                @Override
                public void onAcknowledgePurchaseResponse(BillingResult billingResult) {
                    if (billingResult.getResponseCode() != BillingClient.BillingResponseCode.OK) {
                        Log.w(TAG, "acknowledgePurchase falhou: " + billingResult.getDebugMessage());
                    }
                }
            });
        }
    }

    // Implementado em Rust: crates/client/src/billing_android.rs
    // (Java_quad_1native_TempestBilling_onPurchaseFinished).
    private static native void onPurchaseFinished(String productId, String purchaseToken, int codigo);
}
