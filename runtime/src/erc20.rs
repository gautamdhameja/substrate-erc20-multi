use codec::{Codec, Decode, Encode};
use rstd::prelude::*;
use sr_primitives::traits::{CheckedAdd, CheckedSub, Member, SimpleArithmetic};
use support::{
    decl_event, decl_module, decl_storage, dispatch::Result, ensure, Parameter, StorageMap,
    StorageValue,
};
use system::{self, ensure_signed};

// the module trait
// contains type definitions
pub trait Trait: system::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
    type TokenBalance: Parameter + Member + SimpleArithmetic + Codec + Default + Copy;
}

// struct to store the token details
#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct Erc20Token<U> {
    name: Vec<u8>,
    ticker: Vec<u8>,
    total_supply: U,
}

// public interface for this runtime module
decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
      // initialize the default event for this module
      pub fn deposit_event() = default;

      // initializes a new token
      // generates an integer token_id so that all tokens are unique
      // takes a name, ticker, total supply for the token
      // makes the initiating account the owner of the token
      // the balance of the owner is set to total supply
      fn init(origin, name: Vec<u8>, ticker: Vec<u8>, total_supply: T::TokenBalance) -> Result {
          let sender = ensure_signed(origin)?;

          // checking max size for name and ticker
          // byte arrays (vecs) with no max size should be avoided
          ensure!(name.len() <= 64, "token name cannot exceed 64 bytes");
          ensure!(ticker.len() <= 32, "token ticker cannot exceed 32 bytes");

          let token_id = Self::token_id();
          let next_token_id = token_id.checked_add(1).ok_or("overflow in calculating next token id")?;
          <TokenId>::put(next_token_id);

          let token = Erc20Token {
              name,
              ticker,
              total_supply,
          };

          <Tokens<T>>::insert(token_id, token);
          <BalanceOf<T>>::insert((token_id, sender), total_supply);

          Ok(())
      }

      // transfer tokens from one account to another
      // origin is assumed as sender
      fn transfer(_origin, token_id: u32, to: T::AccountId, value: T::TokenBalance) -> Result {
          let sender = ensure_signed(_origin)?;
          Self::_transfer(token_id, sender, to, value)
      }

      // approve token transfer from one account to another
      // once this is done, transfer_from can be called with corresponding values
      fn approve(_origin, token_id: u32, spender: T::AccountId, value: T::TokenBalance) -> Result {
          let sender = ensure_signed(_origin)?;
          ensure!(<BalanceOf<T>>::exists((token_id, sender.clone())), "Account does not own this token");

          let allowance = Self::allowance((token_id, sender.clone(), spender.clone()));
          let updated_allowance = allowance.checked_add(&value).ok_or("overflow in calculating allowance")?;
          <Allowance<T>>::insert((token_id, sender.clone(), spender.clone()), updated_allowance);

          Self::deposit_event(RawEvent::Approval(token_id, sender.clone(), spender.clone(), value));

          Ok(())
      }

      // the ERC20 standard transfer_from function
      // implemented in the open-zeppelin way - increase/decrease allownace
      // if approved, transfer from an account to another account without owner's signature
      pub fn transfer_from(_origin, token_id: u32, from: T::AccountId, to: T::AccountId, value: T::TokenBalance) -> Result {
        ensure!(<Allowance<T>>::exists((token_id, from.clone(), to.clone())), "Allowance does not exist.");
        let allowance = Self::allowance((token_id, from.clone(), to.clone()));
        ensure!(allowance >= value, "Not enough allowance.");

        // using checked_sub (safe math) to avoid overflow
        let updated_allowance = allowance.checked_sub(&value).ok_or("overflow in calculating allowance")?;
        <Allowance<T>>::insert((token_id, from.clone(), to.clone()), updated_allowance);

        Self::deposit_event(RawEvent::Approval(token_id, from.clone(), to.clone(), value));
        Self::_transfer(token_id, from, to, value)
      }
  }
}

// storage for this module
decl_storage! {
  trait Store for Module<T: Trait> as Erc20 {
      // token id nonce for storing the next token id available for token initialization
      // inspired by the AssetId in the SRML assets module
      TokenId get(token_id): u32;
      // details of the token corresponding to a token id
      Tokens get(token_details): map u32 => Erc20Token<T::TokenBalance>;
      // balances mapping for an account and token
      BalanceOf get(balance_of): map (u32, T::AccountId) => T::TokenBalance;
      // allowance for an account and token
      Allowance get(allowance): map (u32, T::AccountId, T::AccountId) => T::TokenBalance;
  }
}

// events
decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
        Balance = <T as self::Trait>::TokenBalance,
    {
        // event for transfer of tokens
        // tokenid, from, to, value
        Transfer(u32, AccountId, AccountId, Balance),
        // event when an approval is made
        // tokenid, owner, spender, value
        Approval(u32, AccountId, AccountId, Balance),
    }
);

// implementation of mudule
// utility and private functions
// if marked public, accessible by other modules
impl<T: Trait> Module<T> {
    // the ERC20 standard transfer function
    // internal
    fn _transfer(
        token_id: u32,
        from: T::AccountId,
        to: T::AccountId,
        value: T::TokenBalance,
    ) -> Result {
        ensure!(
            <BalanceOf<T>>::exists((token_id, from.clone())),
            "Account does not own this token"
        );
        let sender_balance = Self::balance_of((token_id, from.clone()));
        ensure!(sender_balance >= value, "Not enough balance.");

        let updated_from_balance = sender_balance
            .checked_sub(&value)
            .ok_or("overflow in calculating balance")?;
        let receiver_balance = Self::balance_of((token_id, to.clone()));
        let updated_to_balance = receiver_balance
            .checked_add(&value)
            .ok_or("overflow in calculating balance")?;

        // reduce sender's balance
        <BalanceOf<T>>::insert((token_id, from.clone()), updated_from_balance);

        // increase receiver's balance
        <BalanceOf<T>>::insert((token_id, to.clone()), updated_to_balance);

        Self::deposit_event(RawEvent::Transfer(token_id, from, to, value));
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use primitives::{Blake2Hasher, H256};
    use runtime_io::with_externalities;
    use sr_primitives::weights::Weight;
    use sr_primitives::Perbill;
    use sr_primitives::{
        testing::Header,
        traits::{BlakeTwo256, IdentityLookup},
    };
    use support::{impl_outer_origin, parameter_types};

    impl_outer_origin! {
        pub enum Origin for Test {}
    }

    // For testing the module, we construct most of a mock runtime. This means
    // first constructing a configuration type (`Test`) which `impl`s each of the
    // configuration traits of modules we want to use.
    #[derive(Clone, Eq, PartialEq)]
    pub struct Test;
    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const MaximumBlockWeight: Weight = 1024;
        pub const MaximumBlockLength: u32 = 2 * 1024;
        pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
    }
    impl system::Trait for Test {
        type Origin = Origin;
        type Call = ();
        type Index = u64;
        type BlockNumber = u64;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = u64;
        type Lookup = IdentityLookup<Self::AccountId>;
        type Header = Header;
        type WeightMultiplierUpdate = ();
        type Event = ();
        type BlockHashCount = BlockHashCount;
        type MaximumBlockWeight = MaximumBlockWeight;
        type MaximumBlockLength = MaximumBlockLength;
        type AvailableBlockRatio = AvailableBlockRatio;
        type Version = ();
    }
    impl Trait for Test {
        type Event = ();
        type TokenBalance = u128;
    }
    type TemplateModule = Module<Test>;

    /// test accounts
    const A: u64 = 0;
    const B: u64 = 1;
    const C: u64 = 2;

    // This function basically just builds a genesis storage key/value store according to
    // our desired mockup.
    fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
        system::GenesisConfig::default()
            .build_storage::<Test>()
            .unwrap()
            .into()
    }

    /// send tokens from A to B
    #[test]
    fn xfer() {
        with_externalities(&mut new_test_ext(), || {
            // create a new token as A
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();

            // transfer to B
            TemplateModule::transfer(Origin::signed(A), 0, B, 4).unwrap();

            // A has 6
            assert_eq!(TemplateModule::balance_of((0, A)), 6);

            // B has 4
            assert_eq!(TemplateModule::balance_of((0, B)), 4);
        });
    }

    #[test]
    fn init() {
        with_externalities(&mut new_test_ext(), || {
            assert_eq!(TemplateModule::balance_of((0, A)), 0);
            assert_eq!(TemplateModule::balance_of((1, A)), 0);
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 10);
            assert_eq!(TemplateModule::balance_of((1, A)), 0);
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 10);
            assert_eq!(TemplateModule::balance_of((1, A)), 10);
        });
    }

    #[test]
    fn transfer_pong() {
        with_externalities(&mut new_test_ext(), || {
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 10);
            assert_eq!(TemplateModule::balance_of((0, B)), 0);
            TemplateModule::transfer(Origin::signed(A), 0, B, 1).unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 9);
            assert_eq!(TemplateModule::balance_of((0, B)), 1);
            TemplateModule::transfer(Origin::signed(B), 0, A, 1).unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 10);
            assert_eq!(TemplateModule::balance_of((0, B)), 0);
            TemplateModule::transfer(Origin::signed(A), 0, B, 1).unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 9);
            assert_eq!(TemplateModule::balance_of((0, B)), 1);
            TemplateModule::transfer(Origin::signed(B), 0, A, 1).unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 10);
            assert_eq!(TemplateModule::balance_of((0, B)), 0);
        });
    }

    #[test]
    fn transfer_before_create() {
        with_externalities(&mut new_test_ext(), || {
            TemplateModule::transfer(Origin::signed(A), 0, B, 1).unwrap_err();
            TemplateModule::transfer(Origin::signed(B), 0, A, 1).unwrap_err();
            TemplateModule::transfer(Origin::signed(A), 1, B, 1).unwrap_err();
        });
    }

    #[test]
    fn transfer_none() {
        with_externalities(&mut new_test_ext(), || {
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();
            TemplateModule::transfer(Origin::signed(A), 0, B, 0).unwrap();
        });
    }

    #[test]
    fn transfer_twice() {
        with_externalities(&mut new_test_ext(), || {
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();
            TemplateModule::transfer(Origin::signed(A), 0, B, 5).unwrap();
            TemplateModule::transfer(Origin::signed(A), 0, B, 5).unwrap();
            TemplateModule::transfer(Origin::signed(A), 0, B, 5).unwrap_err();
        });
    }

    #[test]
    fn transfer_overflow() {
        with_externalities(&mut new_test_ext(), || {
            TemplateModule::init(
                Origin::signed(A),
                b"Trash".to_vec(),
                b"TRS".to_vec(),
                u128::max_value(),
            )
            .unwrap();
            TemplateModule::transfer(Origin::signed(A), 0, B, u128::max_value()).unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 0);
            assert_eq!(TemplateModule::balance_of((0, B)), u128::max_value());
        });
    }

    #[test]
    fn transfer_too_much() {
        with_externalities(&mut new_test_ext(), || {
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();
            TemplateModule::transfer(Origin::signed(A), 0, B, 11).unwrap_err();
            assert_eq!(TemplateModule::balance_of((0, A)), 10);
            assert_eq!(TemplateModule::balance_of((0, B)), 0);
        });
    }

    #[test]
    fn transfer_to_self() {
        with_externalities(&mut new_test_ext(), || {
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();
            TemplateModule::transfer(Origin::signed(A), 0, A, 10).unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 10);
        });
    }

    #[test]
    fn transfer_too_much_to_self() {
        with_externalities(&mut new_test_ext(), || {
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();
            TemplateModule::transfer(Origin::signed(A), 0, A, 11).unwrap_err();
            assert_eq!(TemplateModule::balance_of((0, A)), 10);
        });
    }

    #[test]
    fn transfer_zero_to_self() {
        with_externalities(&mut new_test_ext(), || {
            TemplateModule::init(Origin::signed(A), b"Trash".to_vec(), b"TRS".to_vec(), 10)
                .unwrap();
            TemplateModule::transfer(Origin::signed(A), 0, A, 0).unwrap();
            assert_eq!(TemplateModule::balance_of((0, A)), 10);
        });
    }

    #[test]
    fn default_balance_zero() {
        with_externalities(&mut new_test_ext(), || {
            assert_eq!(TemplateModule::balance_of((0, A)), 0);
            assert_eq!(TemplateModule::balance_of((0, B)), 0);
            assert_eq!(TemplateModule::balance_of((0, C)), 0);
            assert_eq!(TemplateModule::balance_of((1, A)), 0);
            assert_eq!(TemplateModule::balance_of((1, B)), 0);
            assert_eq!(TemplateModule::balance_of((1, C)), 0);
        });
    }
}
